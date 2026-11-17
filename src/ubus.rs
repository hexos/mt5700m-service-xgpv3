//! ubus 调用封装：与 QModem 的 at-daemon 通信（UBUS 模式共享串口）。

use std::time::Duration;
use tokio::time::timeout;

#[allow(dead_code)]
pub const AT_DAEMON_OBJECT: &str = "at-daemon";

/// 调用 `ubus call <object> <method> <json>`，返回 JSON 值。
#[allow(dead_code)]
pub async fn ubus_call(
    method: &str,
    params: &serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let payload = serde_json::to_string(params)?;
    let fut = tokio::process::Command::new("ubus")
        .args(["call", AT_DAEMON_OBJECT, method, &payload])
        .output();
    let output = timeout(Duration::from_secs(timeout_secs), fut).await??;

    if !output.status.success() {
        return Err(format!(
            "ubus call {} 失败: {}",
            method,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or(serde_json::json!({}));
    Ok(v)
}
