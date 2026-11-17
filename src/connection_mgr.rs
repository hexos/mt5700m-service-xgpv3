//! 连接管理：connect / disconnect / redial / sync / status / log。
//! 移植自原 luci-app 的 mt5700m-manager（network 同步 + NDIS 会话 + 状态 JSON）。

use serde_json::{json, Map, Value};
use std::process::Command;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::at::ATClient;
use crate::config::Config;
use crate::usb;

pub const INTERFACE: &str = "MT5700M";
pub const INTERFACE6: &str = "MT5700Mv6";

fn run(args: &[&str]) -> String {
    Command::new(args[0])
        .args(&args[1..])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn run_status(args: &[&str]) -> bool {
    Command::new(args[0])
        .args(&args[1..])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn bool_value(v: &str) -> bool {
    v == "true" || v == "1" || v == "True"
}

fn effective_metric(config: &Config) -> u64 {
    // 尊重 h5000m_netmode 的网络优先级策略，与 shell 版本一致。
    let mode = crate::config::uci_get("h5000m_netmode.settings.mode").unwrap_or_default();
    match mode.as_str() {
        "modem_first" | "modem_only" => 10,
        "wan_first" | "wan_only" => 50,
        _ => config.connection.metric,
    }
}

fn saved_profile(config: &Config) -> String {
    let pdp = &config.connection.pdp_type;
    let protocol = match pdp.as_str() {
        "ip" => "IP",
        "ipv6" => "IPV6",
        _ => "IPV4V6",
    };
    let auth = match config.connection.auth.as_str() {
        "pap" => 1,
        "chap" => 2,
        "auto" => 3,
        _ => 0,
    };
    format!(
        "1,1,\"{}\",\"{}\",\"{}\",\"{}\",{}",
        protocol,
        config.connection.apn,
        config.connection.username,
        config.connection.password,
        auth
    )
}

fn profile_valid(p: &str) -> bool {
    // 0 或 7 段 CSV 结构（enable,mode,"PDP","apn","user","pass",auth）。
    if p == "0" {
        return true;
    }
    let parts: Vec<&str> = p.split(',').collect();
    if parts.len() != 7 {
        return false;
    }
    matches!(parts[0], "0" | "1")
        && matches!(parts[1], "1" | "2")
        && matches!(parts[2], "\"IP\"" | "\"IPV6\"" | "\"IPV4V6\"")
        && matches!(parts[6], "0" | "1" | "2" | "3")
}

async fn at_raw(client: &Arc<ATClient>, cmd: &str) -> String {
    client
        .send_command(cmd.to_string())
        .await
        .unwrap_or_default()
}

async fn set_ndis_session(client: &Arc<ATClient>, v: &str) -> bool {
    at_raw(client, &format!("AT^NDISDUP=1,{}", v))
        .await
        .contains("OK")
}

async fn apply_profile(client: &Arc<ATClient>, config: &Config) -> Result<(), String> {
    let desired = saved_profile(config);
    if !profile_valid(&desired) {
        return Err("无效的目标拨号配置".to_string());
    }
    let _ = at_raw(client, "AT^SETAUTODIAL=0").await;
    sleep(Duration::from_secs(1)).await;
    at_raw(client, &format!("AT^SETAUTODIAL={}", desired)).await;
    let current = at_raw(client, "AT^SETAUTODIAL?").await;
    let current = crate::parse::first_match(&current, "^SETAUTODIAL:").unwrap_or_default();
    if crate::parse::clean_value(&current) == desired {
        Ok(())
    } else {
        Err("拨号配置回读校验失败".to_string())
    }
}

async fn bring_up(config: &Config) {
    let _ = run_status(&["ifup", INTERFACE]);
    if config.connection.pdp_type != "ip" {
        let _ = run_status(&["ifup", INTERFACE6]);
    }
}

async fn bring_down() {
    let _ = run_status(&["ifdown", INTERFACE6]);
    let _ = run_status(&["ifdown", INTERFACE]);
}

/// 拨号归属判定：XGPV3 上由 QModem 创建并管理 wwan/mpcie* 接口，
/// 本服务只做面板；否则会和 QModem 抢同一个网卡、重复改写 network UCI。
const DELEGATED_MSG: &str =
    "拨号由 QModem 管理（mt5700m.connection.managed=qmodem），请到 QModem 界面操作";

fn netdev_of(config: &Config) -> Option<String> {
    let override_dev = config.connection.netdev.trim();
    if !override_dev.is_empty() {
        return Some(override_dev.to_string());
    }
    usb::netdev()
}

/// 同步 network UCI 配置（与 shell 版 ensure_network 的核心逻辑一致）。
pub fn sync_network(config: &Config) {
    if config.connection.managed_by_qmodem() {
        crate::log_debug!("[连接] managed=qmodem，跳过 network UCI 同步");
        return;
    }
    let netdev = match netdev_of(config) {
        Some(d) => d,
        None => return,
    };
    let metric = effective_metric(config);
    let pdp = &config.connection.pdp_type;

    let set = |key: &str, val: &str| {
        crate::config::uci_set(key, val);
    };

    set(&format!("network.{}.{}", INTERFACE, "proto"), if pdp == "ipv6" { "none" } else { "dhcp" });
    set(&format!("network.{}.{}", INTERFACE, "managed_by"), "mt5700m");
    set(&format!("network.{}.{}", INTERFACE, "device"), &netdev);
    set(&format!("network.{}.{}", INTERFACE, "ifname"), &netdev);
    set(&format!("network.{}.{}", INTERFACE, "metric"), &metric.to_string());
    set(&format!("network.{}.{}", INTERFACE, "defaultroute"), "1");
    set(&format!("network.{}.{}", INTERFACE, "force_link"), "1");
    set(&format!("network.{}.{}", INTERFACE, "norelease"), "1");

    if pdp == "ip" {
        let _ = crate::config::uci_delete(&format!("network.{}", INTERFACE6));
    } else {
        set(&format!("network.{}.{}", INTERFACE6, "proto"), "dhcpv6");
        set(&format!("network.{}.{}", INTERFACE6, "managed_by"), "mt5700m");
        set(&format!("network.{}.{}", INTERFACE6, "device"), &format!("@{}", INTERFACE));
        set(&format!("network.{}.{}", INTERFACE6, "ifname"), &format!("@{}", INTERFACE));
        set(&format!("network.{}.{}", INTERFACE6, "metric"), &metric.to_string());
        set(&format!("network.{}.{}", INTERFACE6, "auto"), "1");
    }

    crate::config::uci_commit("network");
    let _ = run_status(&["/etc/init.d/network", "reload"]);
}

pub async fn sync(client: &Arc<ATClient>, config: &Config) -> Value {
    let netdev = netdev_of(config).unwrap_or_default();
    let _ = at_raw(client, "AT").await;
    if config.connection.managed_by_qmodem() {
        return json!({
            "success": true,
            "delegated": true,
            "message": DELEGATED_MSG,
            "network": netdev,
            "interface": INTERFACE,
            "interface6": INTERFACE6,
        });
    }
    sync_network(config);
    if config.connection.enabled {
        bring_up(config).await;
    } else {
        bring_down().await;
    }
    json!({
        "success": true,
        "network": netdev,
        "interface": INTERFACE,
        "interface6": INTERFACE6,
    })
}

pub async fn connect(client: &Arc<ATClient>, config: &Config) -> Value {
    if config.connection.managed_by_qmodem() {
        return json!({ "success": false, "delegated": true, "error": DELEGATED_MSG });
    }
    crate::config::uci_set("mt5700m.connection.enabled", "1");
    crate::config::uci_commit("mt5700m");
    sync_network(config);
    if let Err(e) = apply_profile(client, config).await {
        bring_up(config).await;
        return json!({ "success": false, "error": e });
    }
    let _ = set_ndis_session(client, "0").await;
    sleep(Duration::from_secs(2)).await;
    let _ = set_ndis_session(client, "1").await;
    bring_up(config).await;
    json!({ "success": true, "message": "连接已启用" })
}

pub async fn disconnect(client: &Arc<ATClient>, config: &Config) -> Value {
    if config.connection.managed_by_qmodem() {
        return json!({ "success": false, "delegated": true, "error": DELEGATED_MSG });
    }
    crate::config::uci_set("mt5700m.connection.enabled", "0");
    crate::config::uci_commit("mt5700m");
    let _ = set_ndis_session(client, "0").await;
    bring_down().await;
    json!({ "success": true, "message": "连接已断开" })
}

pub async fn redial(client: &Arc<ATClient>, config: &Config) -> Value {
    if config.connection.managed_by_qmodem() {
        return json!({ "success": false, "delegated": true, "error": DELEGATED_MSG });
    }
    bring_down().await;
    let _ = set_ndis_session(client, "0").await;
    sleep(Duration::from_secs(2)).await;
    crate::config::uci_set("mt5700m.connection.enabled", "1");
    crate::config::uci_commit("mt5700m");
    sync_network(config);
    if let Err(e) = apply_profile(client, config).await {
        bring_up(config).await;
        return json!({ "success": false, "error": e });
    }
    let _ = set_ndis_session(client, "1").await;
    bring_up(config).await;
    json!({ "success": true, "message": "连接已重拨" })
}

/// 管理器状态 JSON（无 AT 依赖，供「连接状态」卡片使用）。
pub fn status_json(config: &Config) -> Value {
    let mut m = Map::new();

    let usb_info = usb::usb_info();
    let netdev = netdev_of(config).unwrap_or_default();
    let present = usb::module_present() || !netdev.is_empty();
    let state = usb_info
        .as_ref()
        .map(|i| i.state.as_str().to_string())
        .unwrap_or_else(|| {
            // 没有 USB 节点但能识别到网卡：模组经 MHI/PCIe 转接板枚举。
            if present {
                "pcie".to_string()
            } else {
                "absent".to_string()
            }
        });
    let product = usb_info.as_ref().map(|i| i.product.clone()).unwrap_or_default();
    let slot = usb_info.as_ref().map(|i| i.slot.clone()).unwrap_or_default();
    let at_port = if config.settings.at_port_is_auto() {
        String::new()
    } else {
        config.settings.at_port.clone()
    };

    let up4 = bool_value(
        &run(&[
            "ubus",
            "call",
            &format!("network.interface.{}", INTERFACE),
            "status",
        ])
        .lines()
        .find(|l| l.contains("\"up\""))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().trim_matches(',').to_string())
        .unwrap_or_default(),
    );
    let up6 = bool_value(
        &run(&[
            "ubus",
            "call",
            &format!("network.interface.{}", INTERFACE6),
            "status",
        ])
        .lines()
        .find(|l| l.contains("\"up\""))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().trim_matches(',').to_string())
        .unwrap_or_default(),
    );

    let carrier = if netdev.is_empty() {
        false
    } else {
        std::fs::read_to_string(format!("/sys/class/net/{}/carrier", netdev))
            .map(|s| s.trim() == "1")
            .unwrap_or(false)
    };

    let running = config.connection.enabled && (state == "normal" || state == "pcie");

    m.insert("running".to_string(), Value::from(running));
    m.insert("connected".to_string(), Value::from((up4 || up6) && carrier));
    m.insert("ipv4_up".to_string(), Value::from(up4));
    m.insert("ipv6_up".to_string(), Value::from(up6));
    m.insert("carrier".to_string(), Value::from(carrier));
    m.insert("usb_state".to_string(), Value::from(state));
    m.insert("usb_pid".to_string(), Value::from(product));
    m.insert("usb_slot".to_string(), Value::from(slot));
    m.insert("at_port".to_string(), Value::from(at_port));
    m.insert("network".to_string(), Value::from(netdev));
    m.insert("interface".to_string(), Value::from(INTERFACE));
    m.insert("interface6".to_string(), Value::from(INTERFACE6));
    m.insert("mode".to_string(), Value::from("NCM / ECM"));
    m.insert("metric".to_string(), Value::from(effective_metric(config)));
    m.insert(
        "managed".to_string(),
        Value::from(config.connection.managed.clone()),
    );
    m.insert(
        "at_mode".to_string(),
        Value::from(config.settings.mode.clone()),
    );

    Value::Object(m)
}
