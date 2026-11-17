//! 配置模块：从 OpenWrt UCI 读取 mt5700m 配置（兼容 luci-app-mt5700m 的配置项），
//! 并提供 api 段用于 WebSocket / HTTP 服务参数。

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

#[derive(Debug, Clone)]
pub struct SettingsConfig {
    pub enabled: bool,
    /// auto | serial | network
    pub mode: String,
    pub at_port: String,
    pub host: String,
    pub port: u16,
    pub timeout_s: u64,
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
                host: "192.168.8.1".to_string(),
                port: 20249,
                timeout_s: 8,
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
        c.settings.mode = match mode.as_str() {
            "auto" | "serial" | "network" => mode,
            _ => "auto".to_string(),
        };
        c.settings.at_port = uci_get("mt5700m.settings.at_port").unwrap_or_default();
        c.settings.host = parse_string(
            uci_get("mt5700m.settings.host").as_deref(),
            "192.168.8.1",
        );
        c.settings.port = parse_u64(
            uci_get("mt5700m.settings.port").as_deref(),
            20249,
        ) as u16;
        c.settings.timeout_s = parse_u64(uci_get("mt5700m.settings.timeout").as_deref(), 8);

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
