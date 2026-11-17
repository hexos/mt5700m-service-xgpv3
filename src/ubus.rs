//! QModem `at-daemon` 的 ubus 适配层（XGPV3 推荐路径）。
//!
//! 串口只有一个：谁先 `open()` 谁独占。XGPV3 上 QModem 的 `at-daemon` 已经持有
//! `/dev/ttyUSBx`，本服务**不再自己打开串口**，而是复用 at-daemon 的通道：
//!
//!   - 命令下行：`ubus call at-daemon sendat '{"at_port":..,"at_cmd":..,"timeout":..,"owner":..}'`
//!   - URC 上行：`ubus -S listen qmodem.at.line qmodem.at.urc`
//!   - 端口识别：`ubus call at-daemon list` 中 `is_open=1` 的端口即 QModem 实际持有的 AT 口
//!
//! 只依赖系统自带的 `ubus` 命令行，不引入 libubus 绑定，交叉编译无额外依赖。

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::error::Error;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

/// at-daemon 的 ubus 对象名（QModem 固定）。
pub const AT_DAEMON_OBJECT: &str = "at-daemon";
/// 每一条 AT 收发行的事件名。
pub const EVENT_LINE: &str = "qmodem.at.line";
/// 匹配 URC 前缀的空闲行事件名。
pub const EVENT_URC: &str = "qmodem.at.urc";

type BoxError = Box<dyn Error + Send + Sync>;

/// 清理 at-daemon `raw_line` 字段末尾的 NUL / CR / LF。
fn clean_raw_line(raw: &str) -> String {
    raw.trim_matches(|c| c == '\0' || c == '\r' || c == '\n')
        .to_string()
}

/// 调用 `ubus call at-daemon <method> <json>`，返回解析后的 JSON。
///
/// `timeout_secs` 为 ubus CLI 的整体超时，应略大于方法自身超时；
/// 不设超时时 ubus 默认 30s，会让一次 AT 超时拖死整个请求队列。
pub async fn ubus_call(method: &str, params: &Value, timeout_secs: u64) -> Result<Value, BoxError> {
    let payload = params.to_string();
    let timeout = timeout_secs.max(1).to_string();

    let output = Command::new("ubus")
        .arg("-S")
        .arg("-t")
        .arg(&timeout)
        .arg("call")
        .arg(AT_DAEMON_OBJECT)
        .arg(method)
        .arg(&payload)
        .output()
        .await
        .map_err(|e| format!("无法执行 ubus 命令: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("ubus {} 调用失败（退出码 {:?}）", method, output.status.code())
        } else {
            stderr
        }
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = stdout.trim();
    if text.is_empty() {
        return Err(format!("ubus {} 返回空响应", method).into());
    }
    serde_json::from_str(text)
        .map_err(|e| format!("解析 ubus {} 响应失败: {}（原始: {}）", method, e, text).into())
}

// ========== AT 串口自动识别 ==========

/// `is_open` 可能以数字、布尔或字符串形式出现。
fn port_is_open(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0) > 0,
        Some(Value::String(s)) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// 从 `ubus call at-daemon list` 的回复里挑出最可信的 AT 端口。
///
/// `is_open=1` 表示已被 at-daemon 打开，优先级最高；其次是节点存在的端口。
/// 多个候选时优先保留已配置的 `preferred`，避免重枚举时无谓漂移。
pub fn pick_at_port(reply: &Value, preferred: &str) -> Option<String> {
    let ports = reply.get("ports")?.as_array()?;
    let mut opened: Vec<String> = Vec::new();
    let mut existing: Vec<String> = Vec::new();

    for entry in ports {
        let port = match entry.get("port").and_then(|v| v.as_str()) {
            Some(p) if p.starts_with("/dev/") => p.to_string(),
            _ => continue,
        };
        let open = port_is_open(entry.get("is_open"));
        if open {
            opened.push(port.clone());
        }
        if open || entry.get("file_status").and_then(|v| v.as_str()) == Some("exists") {
            existing.push(port);
        }
    }

    for candidates in [&opened, &existing] {
        if candidates.is_empty() {
            continue;
        }
        if !preferred.is_empty() && candidates.iter().any(|p| p.as_str() == preferred) {
            return Some(preferred.to_string());
        }
        return candidates.first().cloned();
    }
    None
}

fn is_numbered(name: &str, prefix: &str) -> bool {
    match name.strip_prefix(prefix) {
        Some(suffix) => !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// 扫描 `/dev` 下常见 AT 节点，作为 at-daemon 未就绪时的补充。
/// 只枚举节点、不打开端口，避免与 QModem 抢租约；`ttyS*` 排最后。
fn scan_serial_devices() -> Vec<String> {
    let entries = match std::fs::read_dir("/dev") {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut found: Vec<(u8, String)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let tier = if is_numbered(&name, "ttyUSB") {
            0
        } else if is_numbered(&name, "ttyACM") {
            1
        } else if name.starts_with("wwan") && name.contains("at") {
            2
        } else if name.starts_with("mhi_") {
            3
        } else if is_numbered(&name, "ttyS") {
            4
        } else {
            continue;
        };
        found.push((tier, format!("/dev/{}", name)));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, p)| p).collect()
}

/// 单次读取 at-daemon 当前持有的端口（不等待、不扫描节点）。
pub async fn current_at_port(preferred: &str) -> Option<String> {
    let reply = ubus_call("list", &json!({}), 5).await.ok()?;
    pick_at_port(&reply, preferred)
}

/// 重连时重新识别：at-daemon 没报告端口时退回 `/dev` 节点扫描。
pub async fn rediscover_at_port(preferred: &str, fallback: &str) -> Option<String> {
    if let Some(p) = current_at_port(preferred).await {
        return Some(p);
    }
    let devices = scan_serial_devices();
    devices
        .iter()
        .find(|p| p.as_str() == fallback)
        .cloned()
        .or_else(|| devices.first().cloned())
}

/// 启动时自动识别 AT 端口。
///
/// 本服务通常比 QModem 先起来，因此最多等待 `attempts` 轮（每轮 3 秒）；
/// 等不到则扫描 `/dev` 节点，最后用 `fallback` 兜底，保证服务始终能启动。
pub async fn discover_at_port(fallback: &str, attempts: u32) -> (String, &'static str) {
    let total = attempts.max(1);
    for attempt in 0..total {
        if let Some(p) = current_at_port(fallback).await {
            return (p, "at-daemon list");
        }
        if attempt + 1 < total {
            crate::log_info!(
                "[UBUS] 尚未发现 at-daemon 持有的 AT 端口，3 秒后重试 ({}/{})",
                attempt + 1,
                total
            );
            sleep(Duration::from_secs(3)).await;
        }
    }

    let devices = scan_serial_devices();
    if let Some(p) = devices.iter().find(|p| p.as_str() == fallback) {
        return (p.clone(), "/dev 节点（兜底值）");
    }
    if let Some(p) = devices.first() {
        return (p.clone(), "/dev 节点扫描");
    }
    (fallback.to_string(), "配置兜底")
}

/// best-effort 注册 URC 前缀。
///
/// at-daemon 以 (owner, urc_id) 为唯一键；这里用独立 owner，
/// 不会与 QModem 自身的 `qmodem.sms` / `qmodem.voip` 注册冲突。
async fn register_urc_prefixes(at_port: &str, owner: &str) {
    const PREFIXES: &[(&str, &str)] = &[
        ("sms", "+CMT"),
        ("sms-status", "+CDS"),
        ("call-clip", "+CLIP"),
        ("call-ring", "RING"),
        ("call-busy", "BUSY"),
        ("call-nocarrier", "NO CARRIER"),
        ("call-noanswer", "NO ANSWER"),
        ("vendor", "^"),
    ];

    for (id, prefix) in PREFIXES {
        let params = json!({
            "at_port": at_port,
            "owner": owner,
            "urc_id": id,
            "prefix": prefix,
        });
        match ubus_call("urc_register", &params, 5).await {
            Ok(v) if v.get("status").and_then(|s| s.as_str()) == Some("success") => {}
            other => crate::log_debug!("[UBUS] 注册 URC {} 未成功: {:?}", id, other),
        }
    }
    crate::log_info!(
        "[UBUS] URC 前缀注册完成 (at_port={}, owner={})",
        at_port,
        owner
    );
}

/// 把一条事件载荷分发为 URC 广播（去重后）。
fn dispatch_urc(
    payload: &Value,
    urc_tx: &broadcast::Sender<String>,
    seen: &mut VecDeque<(u64, u64)>,
) {
    let raw = match payload.get("raw_line").and_then(|v| v.as_str()) {
        Some(s) => clean_raw_line(s),
        None => return,
    };
    if raw.is_empty() {
        return;
    }

    let epoch = payload
        .get("restart_epoch")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let seq = payload
        .get("sequence")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if epoch > 0 && seq > 0 {
        let key = (epoch, seq);
        if seen.contains(&key) {
            return;
        }
        seen.push_back(key);
        if seen.len() > 512 {
            seen.pop_front();
        }
    }

    // qmodem.at.line 携带 correlation：只有 idle（非命令响应）行才是 URC；
    // qmodem.at.urc 已由 at-daemon 过滤，没有该字段。
    if let Some(corr) = payload.get("correlation").and_then(|v| v.as_str()) {
        if corr != "idle" || !crate::is_urc_line(&raw) {
            return;
        }
    }

    let _ = urc_tx.send(raw);
}

fn event_payload(line: &str, event_name: &str) -> Option<Value> {
    let root: Value = serde_json::from_str(line).ok()?;
    root.get(event_name).cloned()
}

/// 长驻事件监听：订阅 at-daemon 的 line / urc 事件，翻译为 `urc_tx` 广播。
/// ubusd / at-daemon 重启会中断事件流，这里自动重连并重新注册前缀。
pub async fn monitor_events(
    urc_tx: broadcast::Sender<String>,
    owner: String,
    at_port: String,
    auto_detect: bool,
) {
    let mut at_port = at_port;
    let mut seen: VecDeque<(u64, u64)> = VecDeque::new();

    loop {
        if auto_detect {
            if let Some(p) = rediscover_at_port(&at_port, &at_port).await {
                if p != at_port {
                    crate::log_info!("[UBUS] 事件监听 AT 端口更新: {} -> {}", at_port, p);
                    at_port = p;
                }
            }
        }

        crate::log_info!("[UBUS] 启动事件监听 ({} / {})", EVENT_LINE, EVENT_URC);
        register_urc_prefixes(&at_port, &owner).await;

        let mut child = match Command::new("ubus")
            .arg("-S")
            .arg("listen")
            .arg(EVENT_LINE)
            .arg(EVENT_URC)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                crate::log_error!("[UBUS] 无法启动 ubus listen: {}，3 秒后重试", e);
                sleep(Duration::from_secs(3)).await;
                continue;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                let line = match lines.next_line().await {
                    Ok(Some(l)) => l,
                    Ok(None) => break,
                    Err(e) => {
                        crate::log_warn!("[UBUS] 读取事件失败: {}", e);
                        break;
                    }
                };
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(v) = event_payload(&line, EVENT_LINE) {
                    dispatch_urc(&v, &urc_tx, &mut seen);
                    continue;
                }
                if let Some(v) = event_payload(&line, EVENT_URC) {
                    dispatch_urc(&v, &urc_tx, &mut seen);
                }
            }
        }

        let _ = child.wait().await;
        crate::log_warn!("[UBUS] 事件监听中断，3 秒后重连...");
        sleep(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_trailing_nul() {
        assert_eq!(clean_raw_line("RING\0"), "RING");
        assert_eq!(clean_raw_line("+CMTI: \"SM\",1\0"), "+CMTI: \"SM\",1");
    }

    #[test]
    fn parses_line_event_payload() {
        let line = r#"{"qmodem.at.line":{"port":"/dev/ttyUSB0","raw_line":"+CMTI: \"SM\",3","correlation":"idle","sequence":5,"restart_epoch":1}}"#;
        let v = event_payload(line, EVENT_LINE).expect("should parse");
        assert_eq!(v.get("correlation").unwrap().as_str(), Some("idle"));
        assert_eq!(
            v.get("raw_line").unwrap().as_str(),
            Some("+CMTI: \"SM\",3")
        );
    }

    #[test]
    fn ignores_unrelated_event() {
        let line = r#"{"some.other.event":{"x":1}}"#;
        assert!(event_payload(line, EVENT_LINE).is_none());
    }

    #[test]
    fn picks_open_port_from_daemon_list() {
        let reply: Value = serde_json::from_str(
            r#"{"ports":[
                {"port":"/dev/ttyUSB2","is_open":0,"file_status":"missing"},
                {"port":"/dev/ttyUSB1","is_open":1,"fd":7}
            ]}"#,
        )
        .unwrap();
        assert_eq!(pick_at_port(&reply, "").as_deref(), Some("/dev/ttyUSB1"));
    }

    #[test]
    fn prefers_configured_port_when_open() {
        let reply: Value = serde_json::from_str(
            r#"{"ports":[
                {"port":"/dev/ttyUSB1","is_open":1},
                {"port":"/dev/ttyUSB2","is_open":1}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            pick_at_port(&reply, "/dev/ttyUSB2").as_deref(),
            Some("/dev/ttyUSB2")
        );
    }

    #[test]
    fn falls_back_to_existing_node() {
        let reply: Value = serde_json::from_str(
            r#"{"ports":[{"port":"/dev/ttyUSB3","is_open":0,"file_status":"exists"}]}"#,
        )
        .unwrap();
        assert_eq!(pick_at_port(&reply, "").as_deref(), Some("/dev/ttyUSB3"));
    }

    #[test]
    fn skips_missing_ports() {
        let reply: Value = serde_json::from_str(
            r#"{"ports":[{"port":"/dev/ttyUSB9","is_open":0,"file_status":"missing"}]}"#,
        )
        .unwrap();
        assert!(pick_at_port(&reply, "").is_none());
        assert!(pick_at_port(&json!({"ports": []}), "").is_none());
    }
}
