//! AT 客户端：命令收发、响应清洗、结束符判定、心跳探测与模块初始化。

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tokio::time::sleep;

use crate::at::connection::{ATConnection, NetworkATConn, SerialATConn};
use crate::config::Config;

const COMMAND_TIMEOUT: Duration = Duration::from_millis(900);
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

fn normalize_command(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with("\r\n") {
        trimmed.to_string()
    } else {
        format!("{}\r\n", trimmed.trim_end())
    }
}

fn sanitize_response(response: &str, original_cmd: &str) -> String {
    let mut lines: Vec<&str> = response
        .lines()
        .filter(|line| !line.trim().eq_ignore_ascii_case("ping"))
        .collect();

    if let Some(first) = lines.first() {
        let first = first.trim();
        if !original_cmd.is_empty()
            && (first.eq_ignore_ascii_case(original_cmd) || first.starts_with(original_cmd))
        {
            lines.remove(0);
        }
    }

    lines.join("\n").trim().to_string()
}

fn has_terminal_marker(response: &str) -> bool {
    for line in response.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        return line.eq_ignore_ascii_case("OK")
            || line.eq_ignore_ascii_case("ERROR")
            || line.starts_with("+CME ERROR")
            || line.starts_with("+CMS ERROR");
    }
    false
}

fn response_error(response: &str) -> Option<String> {
    for line in response.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("ERROR") {
            return Some("ERROR".to_string());
        }
        if line.starts_with("+CME ERROR") || line.starts_with("+CMS ERROR") {
            return Some(line.to_string());
        }
        return None;
    }
    None
}

async fn drain_stale_data(conn: &mut Box<dyn ATConnection>) {
    let deadline = Instant::now() + Duration::from_millis(50);
    while Instant::now() < deadline {
        match conn.receive().await {
            Ok(data) if data.is_empty() => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}

#[derive(Clone)]
pub struct ATClient {
    pub config: Arc<Config>,
    pub conn: Arc<Mutex<Box<dyn ATConnection>>>,
    pub urc_tx: broadcast::Sender<String>,
}

impl ATClient {
    pub fn new(config: Arc<Config>) -> Result<Self, Box<dyn Error>> {
        let s = &config.settings;

        // 串口直连（serial / auto 已解析出 at_port）或网络 TCP。
        let conn: Box<dyn ATConnection> = if s.mode == "network" {
            Box::new(NetworkATConn::new(config.clone()))
        } else {
            Box::new(SerialATConn::new(config.clone()))
        };

        let (tx, _) = broadcast::channel(1024);
        Ok(Self {
            config,
            conn: Arc::new(Mutex::new(conn)),
            urc_tx: tx,
        })
    }

    pub async fn send_command(
        &self,
        command: String,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        self.send_command_with_timeout(command, COMMAND_TIMEOUT).await
    }

    /// 带自定义超时的 AT 命令收发（SMS 等长事务使用）。
    pub async fn send_command_with_timeout(
        &self,
        command: String,
        timeout_dur: Duration,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut conn = self.conn.lock().await;
        let original_cmd = command.trim().to_string();
        let command = normalize_command(&command);

        if command.is_empty() {
            return Err("EMPTY COMMAND".into());
        }

        drain_stale_data(&mut conn).await;

        crate::log_debug!("[AT] ==> TX: {:?}", command);
        conn.send(command.as_bytes()).await?;

        let mut raw_response = String::new();
        let start = Instant::now();

        while start.elapsed() < timeout_dur {
            match conn.receive().await {
                Ok(data) if !data.is_empty() => {
                    raw_response.push_str(&String::from_utf8_lossy(&data));
                    if has_terminal_marker(&raw_response) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
            sleep(DRAIN_INTERVAL).await;
        }

        let result = sanitize_response(&raw_response, &original_cmd);
        crate::log_debug!("[AT] <== RX: {:?}", result);

        if let Some(err) = response_error(&raw_response) {
            return Err(err.into());
        }

        if result.is_empty() || raw_response.trim().is_empty() {
            return Err("TIMEOUT".into());
        }

        Ok(result)
    }

    pub async fn probe(&self) -> bool {
        let mut conn = self.conn.lock().await;
        if !conn.is_connected() {
            return false;
        }

        drain_stale_data(&mut conn).await;

        if conn.send(b"AT\r\n").await.is_err() {
            return false;
        }

        let mut raw = String::new();
        let start = Instant::now();
        while start.elapsed() < PROBE_TIMEOUT {
            match conn.receive().await {
                Ok(data) if !data.is_empty() => {
                    raw.push_str(&String::from_utf8_lossy(&data));
                    if has_terminal_marker(&raw) {
                        return !raw.trim().is_empty();
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
            sleep(DRAIN_INTERVAL).await;
        }

        crate::log_debug!("[AT] 心跳无响应，残留数据: {:?}", raw.trim());
        false
    }

    pub async fn init_module(&self) {
        let _ = self.send_command("ATE0".into()).await;
        let _ = self.send_command("AT+CNMI=2,1,0,2,0".into()).await;
        let _ = self.send_command("AT+CMGF=0".into()).await;
        let _ = self.send_command("AT+CLIP=1".into()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ping_inside_text() {
        let resp = "+CMGL: 0,0,,20\r\nshipping soon\r\nOK";
        assert!(sanitize_response(resp, "AT+CMGL=4").contains("shipping soon"));
    }

    #[test]
    fn strips_echo_and_ping_echo_lines() {
        let resp = "AT+CMGF?\r\nping\r\n+CMGF: 0\r\nOK";
        assert_eq!(sanitize_response(resp, "AT+CMGF?"), "+CMGF: 0\nOK");
    }

    #[test]
    fn terminal_marker_must_be_last_line() {
        assert!(has_terminal_marker("+CMGF: 0\r\nOK\r\n"));
        assert!(!has_terminal_marker("+CMGL: 1,0,,10\r\nOK inside text\r\n"));
    }

    #[test]
    fn error_only_when_last_line() {
        assert!(response_error("+CMGF: 0\r\nOK").is_none());
        assert_eq!(response_error("+CMGF: 0\r\nERROR").as_deref(), Some("ERROR"));
    }
}
