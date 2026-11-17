//! AT 连接抽象与四种传输实现：串口 / TCP 网络 / tom_modem / at-daemon(ubus)。
//! 与 mt5700webui-openwrt-server 的 at-webserver 保持一致的连接语义，
//! 便于共享串口（QModem / at-daemon）。

use async_trait::async_trait;
use std::error::Error;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::config::Config;

const IDLE_READ_TIMEOUT: Duration = Duration::from_millis(100);

fn should_reset_connection(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
            | ErrorKind::ConnectionRefused
            | ErrorKind::TimedOut
    )
}

async fn write_with_timeout<W>(writer: &mut W, data: &[u8]) -> std::io::Result<usize>
where
    W: AsyncWriteExt + Unpin,
{
    timeout(Duration::from_secs(2), writer.write(data))
        .await
        .unwrap_or_else(|_| Err(std::io::Error::new(ErrorKind::TimedOut, "write timed out")))
}

async fn read_with_timeout<R>(reader: &mut R, timeout_dur: Duration) -> std::io::Result<Vec<u8>>
where
    R: AsyncReadExt + Unpin,
{
    let mut buf = vec![0u8; 1024];
    match timeout(timeout_dur, reader.read(&mut buf)).await {
        Ok(Ok(n)) => {
            if n == 0 {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            buf.truncate(n);
            Ok(buf)
        }
        Ok(Err(err)) => Err(err),
        Err(_) => Err(std::io::Error::new(ErrorKind::TimedOut, "read timed out")),
    }
}

#[async_trait]
pub trait ATConnection: Send {
    async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn send(&mut self, data: &[u8]) -> Result<usize, Box<dyn Error + Send + Sync>>;
    async fn receive(&mut self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>>;
    fn is_connected(&self) -> bool;
    fn disconnect(&mut self);
}

// ========== 串口连接 ==========

pub struct SerialATConn {
    pub config: Arc<Config>,
    stream: Option<SerialStream>,
}

impl SerialATConn {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            stream: None,
        }
    }
}

#[async_trait]
impl ATConnection for SerialATConn {
    async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let s = &self.config.settings;
        let port = tokio_serial::new(&s.at_port, 115200u32)
            .timeout(Duration::from_secs(s.timeout_s))
            .open_native_async()?;
        self.stream = Some(port);
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, Box<dyn Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            match write_with_timeout(stream, data).await {
                Ok(n) => Ok(n),
                Err(err) => {
                    self.stream = None;
                    if should_reset_connection(&err) {
                        return Err("Disconnected".into());
                    }
                    Err(Box::new(err))
                }
            }
        } else {
            Err("Disconnected".into())
        }
    }

    async fn receive(&mut self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            match read_with_timeout(stream, IDLE_READ_TIMEOUT).await {
                Ok(data) => Ok(data),
                Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    Ok(Vec::new())
                }
                Err(err) => {
                    self.stream = None;
                    if should_reset_connection(&err) {
                        return Err("Disconnected".into());
                    }
                    Err(Box::new(err))
                }
            }
        } else {
            Err("Disconnected".into())
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn disconnect(&mut self) {
        self.stream = None;
    }
}

// ========== TCP 网络连接 ==========

pub struct NetworkATConn {
    pub config: Arc<Config>,
    stream: Option<TcpStream>,
}

impl NetworkATConn {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            stream: None,
        }
    }
}

#[async_trait]
impl ATConnection for NetworkATConn {
    async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let s = &self.config.settings;
        let addr = format!("{}:{}", s.host, s.port);
        let stream = timeout(Duration::from_secs(s.timeout_s), TcpStream::connect(addr)).await??;
        self.stream = Some(stream);
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, Box<dyn Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            match write_with_timeout(stream, data).await {
                Ok(n) => Ok(n),
                Err(err) => {
                    self.stream = None;
                    if should_reset_connection(&err) {
                        return Err("Disconnected".into());
                    }
                    Err(Box::new(err))
                }
            }
        } else {
            Err("Disconnected".into())
        }
    }

    async fn receive(&mut self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            match read_with_timeout(stream, IDLE_READ_TIMEOUT).await {
                Ok(data) => Ok(data),
                Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    Ok(Vec::new())
                }
                Err(err) => {
                    self.stream = None;
                    if should_reset_connection(&err) {
                        return Err("Disconnected".into());
                    }
                    Err(Box::new(err))
                }
            }
        } else {
            Err("Disconnected".into())
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn disconnect(&mut self) {
        self.stream = None;
    }
}

// ========== tom_modem 外部命令连接 ==========

pub struct TomModemATConn {
    pub config: Arc<Config>,
    is_connected: bool,
    response: Option<String>,
}

impl TomModemATConn {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            is_connected: false,
            response: None,
        }
    }

    /// 把串口 feature 映射为 tom_modem 的命令行开关（QModem 约定 -u 表示 UBUS）。
    fn feature_flag(feature: &str) -> Option<String> {
        let f = feature.trim();
        if f.is_empty() || f.eq_ignore_ascii_case("NONE") {
            return None;
        }
        if f.eq_ignore_ascii_case("UBUS") || f.eq_ignore_ascii_case("u") {
            return Some("-u".to_string());
        }
        if f.starts_with('-') {
            return Some(f.to_string());
        }
        Some(format!("-{}", f))
    }
}

#[async_trait]
impl ATConnection for TomModemATConn {
    async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.is_connected = true;
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, Box<dyn Error + Send + Sync>> {
        if !self.is_connected {
            return Err("Disconnected".into());
        }
        let command = String::from_utf8_lossy(data).trim().to_string();
        let s = &self.config.settings;
        // tom_modem 第一个参数必须是串口；`auto` 时退回 PCUI 扫描或兜底值。
        let port = if s.at_port_is_auto() {
            crate::usb::pcui_port().unwrap_or_else(|| s.fallback_port())
        } else {
            s.at_port.clone()
        };
        let mut args = vec![port.clone(), "-c".to_string(), command];
        // feature 开关：XGPV3 上 QModem 的 tom_modem 用 `-u` 走 at-daemon 共享串口，
        // 写成 `-UBUS` 会被解析成非法短参数 `-U`。
        if let Some(flag) = Self::feature_flag(&s.tom_modem_feature) {
            args.push(flag);
        }
        let output = timeout(
            Duration::from_secs(s.timeout_s),
            tokio::process::Command::new("tom_modem").args(&args).output(),
        )
        .await??;
        if output.status.success() {
            self.response = Some(String::from_utf8_lossy(&output.stdout).to_string());
            Ok(data.len())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("tom_modem 执行失败: {}", error).into())
        }
    }

    async fn receive(&mut self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .response
            .take()
            .map(String::into_bytes)
            .unwrap_or_default())
    }

    fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn disconnect(&mut self) {
        self.is_connected = false;
    }
}

// ========== at-daemon (ubus) 连接 ==========

/// 通过 `ubus call at-daemon sendat` 与 QModem 共享串口。
///
/// sendat 是「一次调用返回完整响应」，因此把响应缓存到 `response`，
/// 由 `receive()` 一次性取走，复用 `ATClient` 的结束符判定 / 回显清洗逻辑。
/// 本服务只是观察者，不抢租约；遇到 `settings_pending`（QModem 正在初始化）
/// 时短暂重试而不是直接失败。
pub struct UbusATConn {
    pub config: Arc<Config>,
    /// 当前实际使用的 at_port；自动识别模式下随 at-daemon 状态更新。
    at_port: String,
    fallback_port: String,
    auto_detect: bool,
    is_connected: bool,
    response: Option<String>,
}

impl UbusATConn {
    pub fn new(config: Arc<Config>) -> Self {
        let (at_port, fallback_port, auto_detect) = {
            let s = &config.settings;
            let auto = s.at_port_is_auto();
            let current = if auto {
                s.fallback_port()
            } else {
                s.at_port.trim().to_string()
            };
            (current, s.fallback_port(), auto)
        };
        Self {
            config,
            at_port,
            fallback_port,
            auto_detect,
            is_connected: false,
            response: None,
        }
    }

    fn send_timeout(&self) -> u64 {
        self.config.settings.timeout_s.max(2)
    }

    /// 租约 owner 标识，用于 at-daemon 的 at_lease_authorize 鉴权。
    fn owner(&self) -> &'static str {
        "mt5700m-service"
    }

}

#[async_trait]
impl ATConnection for UbusATConn {
    async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // 重连时重新识别：模组重插 / USB 重枚举后 /dev/ttyUSBx 可能变化。
        if self.auto_detect {
            let preferred = self.at_port.clone();
            let fallback = self.fallback_port.clone();
            if let Some(p) = crate::ubus::rediscover_at_port(&preferred, &fallback).await {
                if p != self.at_port {
                    crate::log_info!("[UBUS] AT 端口自动切换: {} -> {}", self.at_port, p);
                    self.at_port = p;
                }
            }
        }

        let params = serde_json::json!({
            "at_port": self.at_port,
            "baudrate": 115200u32,
            "databits": 8,
            "parity": 0,
            "stopbits": 1,
        });
        let reply = crate::ubus::ubus_call("open", &params, self.send_timeout() + 3).await?;
        let status = reply.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status == "success" {
            self.is_connected = true;
            crate::log_info!("[UBUS] 已连接 at-daemon 串口: {} @ 115200 bps", self.at_port);
            Ok(())
        } else {
            let msg = reply
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            self.is_connected = false;
            Err(format!("at-daemon open 失败: {}", msg).into())
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, Box<dyn Error + Send + Sync>> {
        if !self.is_connected {
            return Err("Disconnected".into());
        }
        let command = String::from_utf8_lossy(data)
            .trim_matches(|c| c == '\r' || c == '\n' || c == ' ')
            .to_string();
        if command.is_empty() {
            return Ok(data.len());
        }
        let params = serde_json::json!({
            "at_port": self.at_port,
            "at_cmd": command,
            "timeout": self.send_timeout(),
            "owner": self.owner(),
        });
        let max_retries = 8u32;
        for attempt in 0..max_retries {
            let reply =
                crate::ubus::ubus_call("sendat", &params, self.send_timeout() + 3).await?;
            let status = reply.get("status").and_then(|v| v.as_str()).unwrap_or("");
            match status {
                "success" => {
                    let response = reply
                        .get("response")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    self.response = Some(response);
                    return Ok(data.len());
                }
                "settings_pending" => {
                    if attempt + 1 == max_retries {
                        return Err("AT 端口正被 QModem 初始化占用 (settings_pending)".into());
                    }
                    sleep(Duration::from_millis(250)).await;
                }
                "timeout" => return Err("TIMEOUT".into()),
                "error" => {
                    let msg = reply
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("sendat 失败");
                    return Err(msg.into());
                }
                other => return Err(format!("sendat 未知状态: {}", other).into()),
            }
        }
        Err("sendat 重试耗尽".into())
    }

    async fn receive(&mut self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .response
            .take()
            .map(String::into_bytes)
            .unwrap_or_default())
    }

    fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn disconnect(&mut self) {
        self.is_connected = false;
        self.response = None;
    }
}
