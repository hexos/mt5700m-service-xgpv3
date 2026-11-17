//! 高层查询：组合多次 AT 命令与解析器，产出结构化 JSON。
//! 与原 luci-app 的 mt5700m-at / mt5700m-read 各类 action 一一对应。

use serde_json::{Map, Value};
use std::sync::Arc;

use crate::at::ATClient;
use crate::parse;
use crate::usb;

async fn at(client: &Arc<ATClient>, cmd: &str) -> String {
    client
        .send_command(cmd.to_string())
        .await
        .unwrap_or_default()
}

/// 一组「标签 + 命令」的原始结果（network / system / advanced / sms-info 用）。
async fn labeled_raw(client: &Arc<ATClient>, items: &[(&str, &str)]) -> Value {
    let mut arr = Vec::new();
    for (label, command) in items {
        let raw = at(client, command).await;
        let mut obj = Map::new();
        obj.insert("label".to_string(), Value::from(*label));
        obj.insert("command".to_string(), Value::from(*command));
        obj.insert("raw".to_string(), Value::from(raw));
        arr.push(Value::Object(obj));
    }
    Value::Array(arr)
}

/// 完整状态。
pub async fn status(client: &Arc<ATClient>, signal_cached: Option<Map<String, Value>>) -> Value {
    let mut m = Map::new();
    let cfg = client.config.clone();

    m.insert("enabled".to_string(), Value::from(cfg.settings.enabled));
    m.insert("mode".to_string(), Value::from(cfg.settings.mode.clone()));

    // USB 状态
    if let Some(info) = usb::usb_info() {
        m.insert("usb_state".to_string(), Value::from(info.state.as_str()));
        m.insert("usb_pid".to_string(), Value::from(info.product.clone()));
        m.insert("usb_slot".to_string(), Value::from(info.slot.clone()));
        if let Some(speed) = usb::usb_speed(&info.slot) {
            m.insert("usb_speed_mbps".to_string(), Value::from(speed));
        }
    } else {
        m.insert("usb_state".to_string(), Value::from("absent"));
    }

    let detected_port = usb::pcui_port();
    m.insert(
        "at_port".to_string(),
        Value::from(
            detected_port
                .clone()
                .unwrap_or_else(|| cfg.settings.at_port.clone()),
        ),
    );
    m.insert("host".to_string(), Value::from(cfg.settings.host.clone()));
    m.insert("port".to_string(), Value::from(cfg.settings.port));

    // 通道：network 模式或未检测到串口 → network
    let channel = if cfg.settings.mode == "network" || detected_port.is_none() {
        "network"
    } else {
        "serial"
    };
    m.insert("channel".to_string(), Value::from(channel));

    // 连通性
    let probe = at(client, "AT").await;
    if probe.contains("OK") {
        m.insert("connected".to_string(), Value::from(true));
    } else {
        m.insert("connected".to_string(), Value::from(false));
        return Value::Object(m);
    }

    // 身份
    let ati = at(client, "ATI").await;
    let cgsn = at(client, "AT+CGSN").await;
    let cgsn_imei = cgsn
        .lines()
        .map(|l| l.trim())
        .find(|l| l.len() == 15 && l.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.to_string());
    merge(&mut m, parse::parse_identity(&ati, cgsn_imei.as_deref()));

    // SIM / 运营商 / 号码
    let cpin = at(client, "AT+CPIN?").await;
    let cops = at(client, "AT+COPS?").await;
    let sysinfo = at(client, "AT^SYSINFOEX").await;
    merge(&mut m, parse::parse_sim_operator(&cpin, &cops, &sysinfo));
    let cnum = at(client, "AT+CNUM").await;
    merge(&mut m, parse::parse_subscriber_number(&cnum));

    // 信号（优先缓存，否则实时）
    if let Some(cache) = signal_cached {
        merge(&mut m, cache);
    } else {
        let hcsq = at(client, "AT^HCSQ?").await;
        merge(&mut m, parse::parse_signal(&hcsq));
    }

    // 签约速率
    let mut dsambr = at(client, "AT^DSAMBR=1").await;
    if !dsambr.contains("^DSAMBR:") {
        dsambr = at(client, "AT^DSAMBR=8").await;
    }
    merge(&mut m, parse::parse_subscription_rate(&dsambr));

    // 载波聚合
    let hfreq = at(client, "AT^HFREQINFO?").await;
    let cascell = at(client, "AT^CASCELLINFO?").await;
    let monssc = at(client, "AT^MONSSC").await;
    merge(
        &mut m,
        parse::parse_carrier_aggregation(&hfreq, &cascell, &monssc),
    );

    // 温度
    let chiptemp = at(client, "AT^CHIPTEMP?").await;
    merge(&mut m, parse::parse_temperature(&chiptemp));

    // 锁频状态
    let nrlock = at(client, "AT^NRFREQLOCK?").await;
    let ltelock = at(client, "AT^LTEFREQLOCK?").await;
    merge(&mut m, parse::parse_lock_status(&nrlock, &ltelock));

    Value::Object(m)
}

fn merge(dst: &mut Map<String, Value>, src: Map<String, Value>) {
    for (k, v) in src {
        dst.insert(k, v);
    }
}

/// 实时信号。
pub async fn signal(client: &Arc<ATClient>) -> Value {
    let hcsq = at(client, "AT^HCSQ?").await;
    Value::Object(parse::parse_signal(&hcsq))
}

/// 温度。
pub async fn temperature(client: &Arc<ATClient>) -> Value {
    let chiptemp = at(client, "AT^CHIPTEMP?").await;
    Value::Object(parse::parse_temperature(&chiptemp))
}

/// 网络信息（标签 + 原始输出）。
pub async fn network(client: &Arc<ATClient>) -> Value {
    labeled_raw(
        client,
        &[
            ("Signal", "AT^HCSQ?"),
            ("Serving cell", "AT^MONSC"),
            ("RRC state", "AT^RRCSTAT?"),
            ("Network registration", "AT+CEREG?"),
            ("Operator", "AT+COPS?"),
            ("LTE lock", "AT^LTEFREQLOCK?"),
            ("NR lock", "AT^NRFREQLOCK?"),
        ],
    )
    .await
}

/// 系统信息。
pub async fn system(client: &Arc<ATClient>) -> Value {
    labeled_raw(
        client,
        &[
            ("Identity", "ATI"),
            ("IMEI", "AT+CGSN"),
            ("Revision", "AT+CGMR"),
            ("Version", "AT^VERSION?"),
            ("SIM", "AT+CPIN?"),
            ("ICCID", "AT^ICCID?"),
            ("IMSI", "AT+CIMI"),
            ("Subscriber number", "AT+CNUM"),
            ("Subscription rate", "AT^DSAMBR=1"),
            ("Operator", "AT+COPS?"),
            ("Network time", "AT^NWTIME?"),
            ("Function level", "AT+CFUN?"),
            ("LED", "AT^LEDSWITCH?"),
            ("SIM activation", "AT^HVSST?"),
            ("SIM slot", "AT^SCICHG?"),
            ("FOTA mode", "AT^FOTAMODE?"),
            ("FOTA state", "AT^FOTASTATE?"),
            ("FOTA progress", "AT^FOTADLQ"),
            ("Temperature", "AT^CHIPTEMP?"),
            ("Thermal status", "AT^THERMLDAUTOSTATUS?"),
            ("Thermal thresholds", "AT^THERMLDAUTOPARA?"),
            ("Thermal log", "AT^THERMLDLOGSW?"),
        ],
    )
    .await
}

/// 高级信息分组。
pub async fn advanced(client: &Arc<ATClient>, group: &str) -> Value {
    let items: &[(&str, &str)] = match group {
        "connection" => &[
            ("Auto dial", "AT^SETAUTODIAL?"),
            ("Interface mode", "AT^TDCFG?"),
            ("PDP contexts", "AT+CGDCONT?"),
            ("PDP activation", "AT+CGACT?"),
            ("Data session", "AT^NDISSTATQRY?"),
            ("Detailed sessions", "AT^DCONNSTAT?"),
            ("IPv4 lease", "AT^DHCP?"),
            ("IPv6 lease", "AT^DHCPV6?"),
            ("IP capability", "AT^IPV6CAP?"),
            ("Data flow", "AT^DSFLOWQRY"),
            ("MTU", "AT^CGMTU=1"),
            ("PDP address", "AT+CGPADDR=1"),
        ],
        "connection-settings" => &[
            ("Auto dial", "AT^SETAUTODIAL?"),
            ("Interface mode", "AT^TDCFG?"),
            ("PDP contexts", "AT+CGDCONT?"),
            ("PDP activation", "AT+CGACT?"),
        ],
        "session" => &[
            ("Data session", "AT^NDISSTATQRY?"),
            ("Detailed sessions", "AT^DCONNSTAT?"),
            ("IPv4 lease", "AT^DHCP?"),
            ("IPv6 lease", "AT^DHCPV6?"),
            ("IP capability", "AT^IPV6CAP?"),
            ("Data flow", "AT^DSFLOWQRY"),
            ("MTU", "AT^CGMTU=1"),
            ("PDP address", "AT+CGPADDR=1"),
        ],
        "radio" => &[
            ("Radio mode", "AT^SYSCFGEX?"),
            ("5G access mode", "AT^C5GOPTION?"),
            ("NR carrier aggregation", "AT^NRRCCAPQRY=3"),
            ("VoNR", "AT^NRRCCAPQRY=2"),
            ("DSS", "AT^NRRCCAPQRY=5"),
        ],
        "radio-diagnostics" => &[
            ("LTE secondary cells", "AT^CASCELLINFO?"),
            ("NSA secondary cells", "AT^MONSSC"),
            ("Uplink MCS", "AT^MCS=0"),
            ("Downlink MCS", "AT^MCS=1"),
            ("NR transmit power", "AT^NTXPOWER?"),
            ("NR SSB beam", "AT^NRSSBID?"),
            ("QoS", "AT+CGEQOSRDP=1"),
            ("Data registration", "AT+C5GREG?"),
            ("IMS registration", "AT+CIREG?"),
            ("Dual connectivity", "AT^LENDC?"),
        ],
        "hardware" => &[
            ("USB mode", "AT^SETMODE?"),
            ("Interface mode", "AT^TDCFG?"),
            ("NIC speed", "AT^TDPCIELANCFG?"),
            ("PCIe controller", "AT^TDPMCFG?"),
            ("LED", "AT^LEDSWITCH?"),
            ("SIM hotplug", "AT^TDSIMHP?"),
            ("SIM slot", "AT^SCICHG?"),
            ("Thermal control", "AT^THERMAUTOFUN?"),
        ],
        _ => &[
            ("Auto dial", "AT^SETAUTODIAL?"),
            ("USB mode", "AT^SETMODE?"),
            ("Interface mode", "AT^TDCFG?"),
            ("PDP contexts", "AT+CGDCONT?"),
            ("Radio mode", "AT^SYSCFGEX?"),
            ("NIC speed", "AT^TDPCIELANCFG?"),
            ("PCIe controller", "AT^TDPMCFG?"),
            ("LED", "AT^LEDSWITCH?"),
            ("SIM hotplug", "AT^TDSIMHP?"),
            ("SIM slot", "AT^SCICHG?"),
            ("Thermal control", "AT^THERMAUTOFUN?"),
            ("NR carrier aggregation", "AT^NRRCCAPQRY=3"),
            ("VoNR", "AT^NRRCCAPQRY=2"),
            ("DSS", "AT^NRRCCAPQRY=5"),
        ],
    };
    labeled_raw(client, items).await
}

/// SMS 列表（PDU 模式保存完整负载）。
pub async fn sms_list(client: &Arc<ATClient>) -> Value {
    let cpms = at(client, "AT+CPMS?").await;
    let _ = at(client, "AT+CMGF=0").await;
    let cmgl = at(client, "AT+CMGL=4").await;
    let mut obj = Map::new();
    obj.insert("storage".to_string(), Value::from(cpms));
    obj.insert("messages".to_string(), Value::from(cmgl));
    Value::Object(obj)
}

/// SMS 信息（IMS/服务模式/短消息中心/存储）。
pub async fn sms_info(client: &Arc<ATClient>) -> Value {
    labeled_raw(
        client,
        &[
            ("IMS", "AT^IMSSWITCH?"),
            ("Service mode", "AT+CEUS?"),
            ("SMSC", "AT+CSCA?"),
            ("Storage", "AT+CPMS?"),
        ],
    )
    .await
}
