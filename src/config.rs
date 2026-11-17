//! 配置模块：从 OpenWrt UCI 读取 mt5700m 配置（兼容 luci-app-mt5700m 的配置项），
//! 并提供 api 段用于 WebSocket / HTTP 服务参数。
//!
//! 连接方式（`mt5700m.settings.mode`）：
//!   - `auto`       自动：优先 QModem at-daemon（ubus），其次 PCUI 串口，最后网络 TCP
//!   - `ubus`       经 QModem 的 at-daemon 共享串口（XGPV3 推荐，不与其它拨号程序抢串口）
//!   - `serial`     独占串口（仅在没有 QModem 托管时使用）
//!   - `network`    模组网络 AT（192.168.8.1:20249）
//!   - `tom_modem`  调用 tom_modem 命令行（QModem 自带，`-u` 走 at-daemon）

use std::process::Command;

/// 运行 `uci -q get <key>`，成功返回去除行尾的值。
pub fn uci_get(key: &str) -> Option<String> {
    let out = Command::new("uci").args(["-q", "get", key]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    Some(s)
}

pub fn uci_set(key: &str, value: &str) -> bool {
    Command::new("uci")
        .args(["set", &format!("{}={}", key, value)])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn uci_delete(key: &str) -> bool {
    Command::new("uci")
        .args(["delete", key])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn uci_commit(pkg: &str) -> bool {
    Command::new("uci")
        .args(["commit", pkg])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn parse_bool(v: Option<&str>, default: bool) -> bool {
    match v {
        Some("1") | Some("true") | Some("yes") => true,
        Some("0") | Some("false") | Some("no") => false,
        _ => default,
    }
}

fn parse_u64(v: Option<&str>, default: u64) -> u64 {
    v.and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_string(v: Option<&str>, default: &str) -> String {
    v.map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn normalize_mode(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "ubus" | "at-daemon" | "atdaemon" => "ubus",
        "serial" | "uart" => "serial",
        "network" | "tcp" => "network",
        "tom_modem" | "tommodem" | "tom-modem" => "tom_modem",
        _ => "auto",
    }
    .to_string()
}

#[derive(Debug, Clone)]
pub struct SettingsConfig {
    pub enabled: bool,
    /// auto | ubus | serial | network | tom_modem
    pub mode: String,
    /// 串口路径，或 `auto` / 空 表示自动识别（UBUS 模式由 at-daemon 报告，串口模式扫描 PCUI）
    pub at_port: String,
    /// 自动识别失败时的兜底串口（XGPV3 实测 /dev/ttyUSB1）
    pub at_port_fallback: String,
    pub host: String,
    pub port: u16,
    pub timeout_s: u64,
    /// tom_modem 的串口特性开关，`UBUS` 会被翻译为 `-u`
    pub tom_modem_feature: String,
    /// UBUS 模式下是否仍强行执行 ATE0/CNMI 初始化（默认关闭，避免改动 QModem 的 URC 配置）
    pub force_init: bool,
}

impl SettingsConfig {
    pub fn at_port_is_auto(&self) -> bool {
        let v = self.at_port.trim();
        v.is_empty() || v.eq_ignore_ascii_case("auto") || v.eq_ignore_ascii_case("detect")
    }

    pub fn fallback_port(&self) -> String {
        if !self.at_port_fallback.trim().is_empty() {
            return self.at_port_fallback.trim().to_string();
        }
        if !self.at_port_is_auto() {
            return self.at_port.trim().to_string();
        }
        "/dev/ttyUSB1".to_string()
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub enabled: bool,
    pub apn: String,
    pub pdp_type: String,
    pub auth: String,
    pub username: String,
    pub password: String,
    pub metric: u64,
    pub dns_list: String,
    /// 拨号归属：`qmodem`（默认，交给 QModem，本服务只做面板）或 `self`（本服务自己拨号）
    pub managed: String,
    /// 覆盖自动识别的数据网卡（留空则自动）
    pub netdev: String,
}

impl ConnectionConfig {
    pub fn managed_by_qmodem(&self) -> bool {
        self.managed.eq_ignore_ascii_case("qmodem")
    }
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub websocket_port: u16,
    pub auth_key: String,
    pub http_port: u16,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub settings: SettingsConfig,
    pub connection: ConnectionConfig,
    pub api: ApiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            settings: SettingsConfig {
                enabled: true,
                mode: "auto".to_string(),
                at_port: String::new(),
                at_port_fallback: String::new(),
                host: "192.168.8.1".to_string(),
                port: 20249,
                timeout_s: 8,
                tom_modem_feature: "UBUS".to_string(),
                force_init: false,
            },
            connection: ConnectionConfig {
                enabled: true,
                apn: String::new(),
                pdp_type: "ipv4v6".to_string(),
                auth: "none".to_string(),
                username: String::new(),
                password: String::new(),
                metric: 50,
                dns_list: String::new(),
                managed: "self".to_string(),
                netdev: String::new(),
            },
            api: ApiConfig {
                websocket_port: 8766,
                auth_key: String::new(),
                http_port: 8088,
                enabled: true,
            },
        }
    }
}

impl Config {
    pub fn load_from_uci() -> Config {
        let mut c = Config::default();

        // ---- settings ----
        c.settings.enabled = parse_bool(uci_get("mt5700m.settings.enabled").as_deref(), true);
        let mode = uci_get("mt5700m.settings.mode").unwrap_or_else(|| "auto".to_string());
        c.settings.mode = normalize_mode(&mode);
        c.settings.at_port = uci_get("mt5700m.settings.at_port").unwrap_or_default();
        c.settings.at_port_fallback =
            uci_get("mt5700m.settings.at_port_fallback").unwrap_or_default();
        c.settings.host = parse_string(
            uci_get("mt5700m.settings.host").as_deref(),
            "192.168.8.1",
        );
        c.settings.port = parse_u64(
            uci_get("mt5700m.settings.port").as_deref(),
            20249,
        ) as u16;
        c.settings.timeout_s = parse_u64(uci_get("mt5700m.settings.timeout").as_deref(), 8);
        c.settings.tom_modem_feature = parse_string(
            uci_get("mt5700m.settings.tom_modem_feature").as_deref(),
            "UBUS",
        );
        c.settings.force_init =
            parse_bool(uci_get("mt5700m.settings.force_init").as_deref(), false);

        // ---- connection ----
        c.connection.enabled = parse_bool(uci_get("mt5700m.connection.enabled").as_deref(), true);
        c.connection.apn = uci_get("mt5700m.connection.apn").unwrap_or_default();
        let pdp = uci_get("mt5700m.connection.pdp_type").unwrap_or_else(|| "ipv4v6".to_string());
        c.connection.pdp_type = match pdp.as_str() {
            "ip" | "ipv6" | "ipv4v6" => pdp,
            _ => "ipv4v6".to_string(),
        };
        let auth = uci_get("mt5700m.connection.auth").unwrap_or_else(|| "none".to_string());
        c.connection.auth = match auth.as_str() {
            "none" | "pap" | "chap" | "auto" => auth,
            _ => "none".to_string(),
        };
        c.connection.username = uci_get("mt5700m.connection.username").unwrap_or_default();
        c.connection.password = uci_get("mt5700m.connection.password").unwrap_or_default();
        c.connection.metric = parse_u64(uci_get("mt5700m.connection.metric").as_deref(), 50);
        c.connection.dns_list = uci_get("mt5700m.connection.dns_list").unwrap_or_default();
        let managed = uci_get("mt5700m.connection.managed").unwrap_or_else(|| "self".to_string());
        c.connection.managed = match managed.trim().to_ascii_lowercase().as_str() {
            "qmodem" | "qmodem_mwan" | "external" => "qmodem".to_string(),
            _ => "self".to_string(),
        };
        c.connection.netdev = uci_get("mt5700m.connection.netdev").unwrap_or_default();

        // ---- api ----
        c.api.enabled = parse_bool(uci_get("mt5700m.api.enabled").as_deref(), true);
        c.api.websocket_port = parse_u64(
            uci_get("mt5700m.api.websocket_port").as_deref(),
            8766,
        ) as u16;
        c.api.auth_key = uci_get("mt5700m.api.auth_key").unwrap_or_default();
        c.api.http_port = parse_u64(uci_get("mt5700m.api.http_port").as_deref(), 8088) as u16;

        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_aliases() {
        assert_eq!(normalize_mode("UBUS"), "ubus");
        assert_eq!(normalize_mode("at-daemon"), "ubus");
        assert_eq!(normalize_mode("tom-modem"), "tom_modem");
        assert_eq!(normalize_mode("tcp"), "network");
        assert_eq!(normalize_mode("bogus"), "auto");
    }

    #[test]
    fn auto_port_detection() {
        let mut s = SettingsConfig {
            enabled: true,
            mode: "ubus".into(),
            at_port: "auto".into(),
            at_port_fallback: "/dev/ttyUSB1".into(),
            host: String::new(),
            port: 0,
            timeout_s: 8,
            tom_modem_feature: "UBUS".into(),
            force_init: false,
        };
        assert!(s.at_port_is_auto());
        assert_eq!(s.fallback_port(), "/dev/ttyUSB1");
        s.at_port = "/dev/ttyUSB3".into();
        assert!(!s.at_port_is_auto());
        s.at_port_fallback.clear();
        assert_eq!(s.fallback_port(), "/dev/ttyUSB3");
    }

    #[test]
    fn qmodem_managed_flag() {
        let mut c = Config::default();
        assert!(!c.connection.managed_by_qmodem());
        c.connection.managed = "QModem".into();
        assert!(c.connection.managed_by_qmodem());
    }
}
