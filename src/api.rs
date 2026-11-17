//! API 层：WebSocket（原生 AT 透传 + 结构化 JSON 命令）+ 最小 HTTP 端点。
//! WebSocket 协议是 mt5700webui-openwrt-server at-webserver 协议的超集：
//!   - 纯文本 "ping" → "pong"
//!   - 纯文本 "AT..." → {"success":bool,"data":...}
//!   - JSON {"action":...} → 结构化结果
//!   - 服务端 URC 推送 {"type":"raw_data","data":...}

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::at::ATClient;
use crate::config::Config;
use crate::signal::SignalCache;
use crate::traffic::TrafficState;

pub struct SharedState {
    pub config: Arc<Config>,
    pub client: Arc<ATClient>,
    pub signal_cache: SignalCache,
    pub traffic_state: TrafficState,
}

fn get_str(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn get_u8(v: &Value, key: &str) -> u8 {
    v.get(key)
        .and_then(|x| x.as_u64())
        .unwrap_or(0) as u8
}

fn ok(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null })
}

fn fail(err: String) -> Value {
    json!({ "success": false, "data": null, "error": err })
}

/// 结构化 JSON 命令分发。
async fn handle_action(state: &Arc<SharedState>, req: &Value) -> Value {
    let client = &state.client;
    let config = &state.config;
    let action = get_str(req, "action");

    match action.as_str() {
        // ---- 配置读写 ----
        "config" => {
            let c = &state.config;
            ok(json!({
                "settings": {
                    "enabled": c.settings.enabled,
                    "mode": c.settings.mode,
                    "at_port": c.settings.at_port,
                    "host": c.settings.host,
                    "port": c.settings.port,
                    "timeout": c.settings.timeout_s
                },
                "connection": {
                    "enabled": c.connection.enabled,
                    "apn": c.connection.apn,
                    "pdp_type": c.connection.pdp_type,
                    "auth": c.connection.auth,
                    "username": c.connection.username,
                    "password": c.connection.password,
                    "metric": c.connection.metric,
                    "dns_list": c.connection.dns_list
                },
                "api": {
                    "enabled": c.api.enabled,
                    "websocket_port": c.api.websocket_port,
                    "auth_key": c.api.auth_key,
                    "http_port": c.api.http_port
                }
            }))
        }
        "config-set" => {
            let section = get_str(req, "section");
            let key = get_str(req, "key");
            let value = get_str(req, "value");
            let uci_key = format!("mt5700m.{}.{}", section, key);
            if crate::config::uci_set(&uci_key, &value) && crate::config::uci_commit("mt5700m") {
                ok(json!({ "saved": true }))
            } else {
                fail("配置保存失败".to_string())
            }
        }

        // ---- 查询 ----
        "status" => {
            let cache = crate::signal::read_cached(&state.signal_cache).await;
            ok(crate::queries::status(client, cache).await)
        }
        "signal" => {
            let cache = crate::signal::read_cached(&state.signal_cache).await;
            match cache {
                Some(m) => ok(Value::Object(m)),
                None => ok(crate::queries::signal(client).await),
            }
        }
        "temperature" => ok(crate::queries::temperature(client).await),
        "network" => ok(crate::queries::network(client).await),
        "system" => ok(crate::queries::system(client).await),
        "advanced" => {
            let group = get_str(req, "group");
            ok(crate::queries::advanced(client, &group).await)
        }
        "sms-list" => ok(crate::queries::sms_list(client).await),
        "sms-info" => ok(crate::queries::sms_info(client).await),
        "traffic" => ok(crate::traffic::export_json(&state.traffic_state).await),
        "connection-status" => ok(crate::connection_mgr::status_json(config)),
        "log" => ok(read_log()),
        "at" => {
            let cmd = get_str(req, "command");
            match client.send_command(cmd).await {
                Ok(r) => ok(Value::String(r)),
                Err(e) => fail(e.to_string()),
            }
        }

        // ---- 连接管理 ----
        "connect" => ok(crate::connection_mgr::connect(client, config).await),
        "disconnect" => ok(crate::connection_mgr::disconnect(client, config).await),
        "redial" => ok(crate::connection_mgr::redial(client, config).await),
        "sync" => ok(crate::connection_mgr::sync(client, config).await),

        // ---- 写操作 ----
        "lock" => act(
            crate::actions::lock(
                client,
                &get_str(req, "rat"),
                get_u8(req, "type"),
                &get_str(req, "bands"),
                &get_str(req, "arfcns"),
                &get_str(req, "scs"),
                &get_str(req, "pcis"),
            )
            .await,
        ),
        "unlock" => act(crate::actions::unlock(client).await),
        "airplane" => act(crate::actions::airplane(client, &get_str(req, "state")).await),
        "restart" => act(crate::actions::restart(client).await),
        "radio-mode" => act(crate::actions::radio_mode(client, &get_str(req, "value")).await),
        "radio-policy" => act(
            crate::actions::radio_policy(
                client,
                &get_str(req, "acqorder"),
                &get_str(req, "band"),
                &get_str(req, "roam"),
                &get_str(req, "srvdomain"),
                &get_str(req, "lteband"),
            )
            .await,
        ),
        "5g-access" => act(crate::actions::set_5g_access_mode(client, &get_str(req, "preset")).await),
        "autodial" => act(
            crate::actions::autodial(
                client,
                &get_str(req, "enable"),
                &get_str(req, "dial_mode"),
                &get_str(req, "protocol"),
                &get_str(req, "apn"),
                &get_str(req, "username"),
                &get_str(req, "password"),
                &get_str(req, "auth_type"),
            )
            .await,
        ),
        "nic-speed" => act(crate::actions::nic_speed(client, &get_str(req, "value")).await),
        "pcie-controller" => {
            act(crate::actions::pcie_controller(client, &get_str(req, "value")).await)
        }
        "led" => act(crate::actions::led(client, &get_str(req, "value")).await),
        "usb-mode" => act(crate::actions::usb_mode(client, &get_str(req, "value")).await),
        "interface-mode" => {
            act(crate::actions::interface_mode(client, &get_str(req, "value")).await)
        }
        "postroute" => act(crate::actions::postroute(client, &get_str(req, "value")).await),
        "dmz" => act(crate::actions::dmz(client, &get_str(req, "value")).await),
        "sim-hotplug" => act(crate::actions::sim_hotplug(client, &get_str(req, "value")).await),
        "sim-activation" => {
            act(crate::actions::sim_activation(client, &get_str(req, "value")).await)
        }
        "sim-slot" => act(crate::actions::sim_slot(client, &get_str(req, "value")).await),
        "thermal" => act(
            crate::actions::thermal(client, &get_str(req, "enabled"), &get_str(req, "interval"))
                .await,
        ),
        "thermal-log" => act(
            crate::actions::thermal_log(client, &get_str(req, "serial"), &get_str(req, "file"))
                .await,
        ),
        "thermal-thresholds" => {
            let vals: Vec<String> = (1..=9)
                .map(|i| get_str(req, &format!("v{}", i)))
                .collect();
            act(crate::actions::thermal_thresholds(client, &vals).await)
        }
        "carrier-aggregation" => {
            act(crate::actions::carrier_aggregation(client, &get_str(req, "value")).await)
        }
        "vonr" => act(crate::actions::vonr(client, &get_str(req, "value")).await),
        "dss" => act(
            crate::actions::dss(client, &get_str(req, "rate"), &get_str(req, "dmrs")).await,
        ),
        "pdp-set" => act(
            crate::actions::pdp_set(
                client,
                &get_str(req, "cid"),
                &get_str(req, "pdp_type"),
                &get_str(req, "apn"),
            )
            .await,
        ),
        "pdp-remove" => act(crate::actions::pdp_remove(client, &get_str(req, "cid")).await),
        "pdp-state" => act(
            crate::actions::pdp_state(client, &get_str(req, "state"), &get_str(req, "cid")).await,
        ),
        "ndis" => act(crate::actions::ndis(client, &get_str(req, "value")).await),
        "flow-clear" => act(crate::actions::flow_clear(client).await),
        "sim-pin" => act(
            crate::actions::sim_pin(
                client,
                &get_str(req, "operation"),
                &get_str(req, "a"),
                &get_str(req, "b"),
            )
            .await,
        ),
        "sms-ims" => act(crate::actions::sms_ims(client, &get_str(req, "state")).await),
        "factory-reset" => act(crate::actions::factory_reset(client).await),
        "set-imei" => act(crate::actions::set_imei(client, &get_str(req, "imei")).await),
        "fota-init" => act(crate::actions::fota_init(client).await),
        "fota-state" => act(crate::actions::fota_state(client).await),
        "fota-progress" => act(crate::actions::fota_progress(client).await),
        "fota-download" => act(crate::actions::fota_download(client, &get_str(req, "url")).await),
        "fota-start" => act(crate::actions::fota_start(client, &get_str(req, "url")).await),
        "fota-resume" => act(crate::actions::fota_resume(client).await),
        "fota-upgrade" => act(crate::actions::fota_upgrade(client).await),
        "sms-send" => act(
            crate::sms::send_sms(client, &get_str(req, "number"), &get_str(req, "text")).await,
        ),
        "sms-delete" => act(crate::actions::sms_delete(client, &get_str(req, "index")).await),
        "sms-clear" => act(crate::actions::sms_clear(client).await),
        "sms-set" => act(
            crate::actions::sms_set(client, &get_str(req, "key"), &get_str(req, "value")).await,
        ),

        _ => fail(format!("未知操作: {}", action)),
    }
}

fn act(r: Result<String, String>) -> Value {
    match r {
        Ok(s) => ok(Value::String(s)),
        Err(e) => fail(e),
    }
}

fn read_log() -> Value {
    let path = crate::config::uci_get("mt5700m.api.log_file")
        .unwrap_or_else(|| "/tmp/mt5700m-service.log".to_string());
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let lines: Vec<&str> = s.lines().rev().take(120).collect();
            Value::String(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
        }
        Err(_) => Value::String(String::new()),
    }
}

/// WebSocket 连接处理。
pub async fn handle_ws(
    stream: TcpStream,
    addr: std::net::SocketAddr,
    state: Arc<SharedState>,
) -> Option<()> {
    let ws_stream = accept_async(stream).await.ok()?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    let mut urc_rx = state.client.urc_tx.subscribe();

    crate::log_info!("[WS] 新连接: {}", addr);

    // 认证（可选）。
    if !state.config.api.auth_key.is_empty() {
        let key = state.config.api.auth_key.clone();
        let auth_ok = timeout(Duration::from_secs(10), async {
            if let Some(Ok(Message::Text(m))) = ws_rx.next().await {
                if let Ok(v) = serde_json::from_str::<Value>(&m) {
                    if v.get("auth_key").and_then(|x| x.as_str()) == Some(&key) {
                        return true;
                    }
                }
            }
            false
        })
        .await
        .unwrap_or(false);

        if !auth_ok {
            let _ = ws_tx
                .send(Message::Text(
                    json!({ "error": "Authentication failed", "message": "密钥验证失败" }).to_string(),
                ))
                .await;
            return None;
        }
        let _ = ws_tx
            .send(Message::Text(
                json!({ "success": true, "message": "认证成功" }).to_string(),
            ))
            .await;
    }

    loop {
        tokio::select! {
            urc_res = urc_rx.recv() => {
                if let Ok(msg) = urc_res {
                    let payload = json!({ "type": "raw_data", "data": msg });
                    if let Ok(s) = serde_json::to_string(&payload) {
                        if ws_tx.send(Message::Text(s)).await.is_err() { break; }
                    }
                }
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(cmd))) => {
                        let reply = if cmd.trim().eq_ignore_ascii_case("ping") {
                            "pong".to_string()
                        } else if cmd.trim_start().starts_with('{') {
                            match serde_json::from_str::<Value>(&cmd) {
                                Ok(req) => serde_json::to_string(&handle_action(&state, &req).await)
                                    .unwrap_or_else(|_| fail("序列化失败".to_string()).to_string()),
                                Err(_) => fail("无效 JSON".to_string()).to_string(),
                            }
                        } else {
                            let res = match state.client.send_command(cmd).await {
                                Ok(r) => json!({ "success": true, "data": r, "error": null }),
                                Err(e) => json!({ "success": false, "data": null, "error": e.to_string() }),
                            };
                            serde_json::to_string(&res).unwrap_or_default()
                        };
                        if ws_tx.send(Message::Text(reply)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
    crate::log_info!("[WS] 连接断开: {}", addr);
    Some(())
}

/// 定位前端静态资源根目录（优先环境变量，其次 OpenWrt 安装路径，最后本地 web/）。
fn web_root() -> Option<PathBuf> {
    let candidates: Vec<String> = [
        std::env::var("MT5700M_WEB_ROOT").ok(),
        Some("/usr/share/mt5700m/web".to_string()),
        Some("web".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect();
    for c in candidates {
        let p = Path::new(&c);
        if p.join("index.html").is_file() {
            return Some(p.to_path_buf());
        }
    }
    None
}

fn mime_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn http_response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    )
    .into_bytes();
    resp.extend_from_slice(body);
    resp
}

/// HTTP 处理：GET /api/<action>（只读）+ 前端静态文件托管。
pub async fn handle_http(stream: TcpStream, state: Arc<SharedState>) {
    let mut buf = [0u8; 2048];
    let mut reader = stream;
    let _ = timeout(Duration::from_secs(2), reader.read(&mut buf)).await;
    let req = String::from_utf8_lossy(&buf);
    let first_line = req.lines().next().unwrap_or("");

    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let path = path.split('?').next().unwrap_or("/").to_string();

    // ---- /api/* ----
    if let Some(action) = path.strip_prefix("/api/") {
        let body = match action {
            "status" | "signal" | "temperature" | "network" | "system" | "traffic"
            | "connection-status" | "log" | "config" => {
                let v = handle_action(&state, &json!({ "action": action })).await;
                serde_json::to_string(&v).unwrap_or_default()
            }
            _ => serde_json::to_string(&fail("未知端点".to_string())).unwrap_or_default(),
        };
        let resp = http_response("200 OK", "application/json", body.as_bytes());
        let _ = reader.write_all(&resp).await;
        return;
    }

    // ---- 静态文件 ----
    let root = web_root();
    let mut rel = path.trim_start_matches('/').to_string();
    if rel.is_empty() {
        rel = "index.html".to_string();
    }
    let resp = match root {
        Some(root) => {
            let file = root.join(&rel);
            if let Ok(canon) = file.canonicalize() {
                let canon_root = root.canonicalize().unwrap_or_else(|_| root.clone());
                if canon.starts_with(&canon_root) && canon.is_file() {
                    match std::fs::read(&canon) {
                        Ok(content) => http_response("200 OK", mime_type(&rel), &content),
                        Err(_) => http_response("404 Not Found", "text/plain", b"not found"),
                    }
                } else {
                    http_response("404 Not Found", "text/plain", b"not found")
                }
            } else {
                http_response("404 Not Found", "text/plain", b"not found")
            }
        }
        None => http_response("404 Not Found", "text/plain", b"web root not found"),
    };
    let _ = reader.write_all(&resp).await;
}
