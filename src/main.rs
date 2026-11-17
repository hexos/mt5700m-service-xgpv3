//! MT5700M 管理服务入口：加载配置、建立 AT 连接、启动采集与 API 服务。

mod actions;
mod api;
mod at;
mod config;
mod connection_mgr;
mod logger;
mod net_utils;
mod parse;
mod queries;
mod signal;
mod sms;
mod traffic;
mod ubus;
mod usb;

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

use at::ATClient;

pub fn is_urc_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("ping")
        || trimmed.eq_ignore_ascii_case("pong")
        || trimmed
            .as_bytes()
            .windows(4)
            .any(|w| w.eq_ignore_ascii_case(b"ping"))
    {
        return false;
    }
    let upper = trimmed.to_ascii_uppercase();
    trimmed.starts_with('^')
        || trimmed.starts_with('+')
        || upper.starts_with("RING")
        || upper.starts_with("NO CARRIER")
        || upper.starts_with("NO ANSWER")
        || upper.starts_with("BUSY")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = config::Config::load_from_uci();
    logger::init(true, "INFO");

    // 解析连接模式。
    //   auto → 优先 QModem at-daemon（XGPV3 上串口被 QModem 独占，必须共享）
    //          → 其次 PCUI 串口 → 最后网络 TCP
    //   ubus → 等待 at-daemon 就绪并自动识别 AT 端口
    let auto_port = config.settings.at_port_is_auto();
    let fallback_port = config.settings.fallback_port();
    // 先取出 mode 的副本再 match：直接用 `config.settings.mode.as_str()` 会在整个
    // match 期间持有不可变借用，导致各分支里无法写回 mode（E0506）。
    let mode = config.settings.mode.clone();
    match mode.as_str() {
        "auto" => {
            if ubus::current_at_port(&fallback_port).await.is_some() {
                log_info!("[连接] 检测到 QModem at-daemon，使用 UBUS 共享串口");
                config.settings.mode = "ubus".to_string();
            } else if let Some(port) = usb::pcui_port() {
                log_info!("[连接] 检测到 PCUI AT 端口: {}", port);
                config.settings.mode = "serial".to_string();
                config.settings.at_port = port;
            } else {
                log_info!(
                    "[连接] 未检测到 PCUI 端口，使用网络模式 {}:{}",
                    config.settings.host,
                    config.settings.port
                );
                config.settings.mode = "network".to_string();
            }
        }
        "serial" => {
            if auto_port {
                config.settings.at_port = usb::pcui_port().unwrap_or_else(|| fallback_port.clone());
            }
        }
        "ubus" => {
            if auto_port {
                let (port, source) = ubus::discover_at_port(&fallback_port, 5).await;
                log_info!("[UBUS] 自动识别 AT 端口: {} (来源: {})", port, source);
                config.settings.at_port = port;
            }
        }
        _ => {}
    }

    log_info!("连接模式: {}", config.settings.mode);
    if config.settings.mode == "ubus" {
        log_info!("[UBUS] AT 端口: {}（自动识别: {}）", config.settings.at_port, auto_port);
    }
    let config = Arc::new(config);

    let client = Arc::new(ATClient::new(config.clone())?);

    // UBUS 模式：URC 只能从 at-daemon 的事件总线取（串口被 QModem 独占，
    // 本服务不能自己读串口，否则会把 QModem 的响应读走）。
    if config.settings.mode == "ubus" {
        let tx = client.urc_tx.clone();
        let port = config.settings.at_port.clone();
        tokio::spawn(async move {
            ubus::monitor_events(tx, "mt5700m-service".to_string(), port, auto_port).await;
        });
    }

    // 初次连接 + 模块初始化。
    {
        let c = client.clone();
        let mut conn = c.conn.lock().await;
        if let Err(e) = conn.connect().await {
            log_warn!("[AT] 初始连接失败: {}", e);
        } else {
            drop(conn);
            let c2 = c.clone();
            tokio::spawn(async move {
                c2.init_module().await;
            });
        }
    }

    // 心跳。
    {
        let c = client.clone();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(30));
            let mut failures = 0u32;
            loop {
                ticker.tick().await;
                if c.probe().await {
                    failures = 0;
                } else {
                    failures += 1;
                    if failures >= 3 {
                        log_warn!("[AT] 连续心跳失败，标记断开等待重连");
                        c.conn.lock().await.disconnect();
                        failures = 0;
                    }
                }
            }
        });
    }

    // 连接维护 + URC 捕获。
    {
        let c = client.clone();
        tokio::spawn(async move {
            let mut last_reconnect = std::time::Instant::now();
            let mut was_connected = false;
            loop {
                let mut has_data = false;
                {
                    let mut conn = c.conn.lock().await;
                    if !conn.is_connected() {
                        if was_connected {
                            log_warn!("[AT] 连接断开，正在重连...");
                            was_connected = false;
                        }
                        if last_reconnect.elapsed() >= Duration::from_secs(2) {
                            last_reconnect = std::time::Instant::now();
                            if conn.connect().await.is_ok() {
                                log_info!("[AT] 连接已恢复");
                                was_connected = true;
                                drop(conn);
                                let c2 = c.clone();
                                tokio::spawn(async move {
                                    c2.init_module().await;
                                });
                            }
                        }
                    } else {
                        match conn.receive().await {
                            Ok(data) if !data.is_empty() => {
                                has_data = true;
                                let text = String::from_utf8_lossy(&data).to_string();
                                for line in text.lines() {
                                    if is_urc_line(line) {
                                        log_debug!("[URC] <== {:?}", line);
                                        let _ = c.urc_tx.send(line.to_string());
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(_) => {
                                if was_connected {
                                    log_warn!("[AT] 连接断开，正在重连...");
                                    was_connected = false;
                                }
                            }
                        }
                    }
                }
                if !has_data {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        });
    }

    // 信号采集 + 流量统计。
    let signal_cache = signal::new_cache();
    {
        let c = client.clone();
        let cache = signal_cache.clone();
        tokio::spawn(async move {
            signal::collector_loop(c, cache).await;
        });
    }
    let traffic_state = traffic::new_state();
    {
        let s = traffic_state.clone();
        tokio::spawn(async move {
            traffic::collector_loop(s).await;
        });
    }

    let state = Arc::new(api::SharedState {
        config: config.clone(),
        client: client.clone(),
        signal_cache,
        traffic_state,
    });

    // HTTP 服务。
    let http_port = config.api.http_port;
    {
        let s = state.clone();
        tokio::spawn(async move {
            match net_utils::create_ipv4_listener("0.0.0.0", http_port).await {
                Ok(listener) => {
                    log_info!("HTTP API 监听: 0.0.0.0:{}", http_port);
                    loop {
                        if let Ok((stream, _addr)) = listener.accept().await {
                            let s = s.clone();
                            tokio::spawn(async move {
                                api::handle_http(stream, s).await;
                            });
                        }
                    }
                }
                Err(e) => log_warn!("HTTP API 绑定失败: {}", e),
            }
        });
    }

    // WebSocket 服务（优先 IPv6 双栈，回退 IPv4）。
    let ws_port = config.api.websocket_port;
    let ws_listener = match net_utils::create_dual_stack_listener("::", ws_port).await {
        Ok(l) => l,
        Err(e) => {
            log_warn!("IPv6 双栈绑定失败: {}，回退 IPv4", e);
            net_utils::create_ipv4_listener("0.0.0.0", ws_port).await?
        }
    };
    log_info!("WebSocket 服务监听端口: {}", ws_port);
    log_info!("MT5700M 服务启动完成");

    loop {
        match ws_listener.accept().await {
            Ok((stream, addr)) => {
                let s = state.clone();
                tokio::spawn(async move {
                    let _ = api::handle_ws(stream, addr, s).await;
                });
            }
            Err(e) => {
                log_error!("接受连接失败: {}", e);
                break;
            }
        }
    }

    Ok(())
}
