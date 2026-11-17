//! 流量统计：读取网络接口 rx/tx 字节计数，累计总/日/月流量，导出 JSON。
//! 移植自原 luci-app 的 mt5700m-traffic。

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

const SYS_CLASS_NET: &str = "/sys/class/net";

#[derive(Debug, Clone, Default)]
pub struct InterfaceStats {
    pub rx_total: u64,
    pub tx_total: u64,
    pub day_rx: u64,
    pub day_tx: u64,
    pub month_rx: u64,
    pub month_tx: u64,
    pub day_key: String,
    pub month_key: String,
}

pub type TrafficState = Arc<Mutex<HashMap<String, InterfaceStats>>>;

pub fn new_state() -> TrafficState {
    Arc::new(Mutex::new(HashMap::new()))
}

fn now_keys() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%Y-%m-%d").to_string(),
        now.format("%Y-%m").to_string(),
    )
}

fn resolve_interface() -> Option<String> {
    if let Some(dev) = crate::config::uci_get("network.MT5700M.device") {
        if std::path::Path::new(SYS_CLASS_NET).join(&dev).is_dir() {
            return Some(dev);
        }
    }
    if let Some(dev) = crate::usb::netdev() {
        return Some(dev);
    }
    for cand in ["eth2", "wwan0", "usb0"] {
        if std::path::Path::new(SYS_CLASS_NET).join(cand).is_dir() {
            return Some(cand.to_string());
        }
    }
    None
}

fn read_counter(iface: &str, file: &str) -> Option<u64> {
    std::fs::read_to_string(format!("{}/{}/statistics/{}", SYS_CLASS_NET, iface, file))
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

async fn collect_once(state: &TrafficState, last: &mut HashMap<String, (u64, u64)>) {
    let Some(iface) = resolve_interface() else { return };
    let (Some(rx), Some(tx)) = (read_counter(&iface, "rx_bytes"), read_counter(&iface, "tx_bytes"))
    else {
        last.remove(&iface);
        return;
    };

    let (day_key, month_key) = now_keys();

    let mut rx_delta = rx;
    let mut tx_delta = tx;
    if let Some((lr, lt)) = last.get(&iface) {
        rx_delta = rx.saturating_sub(*lr);
        tx_delta = tx.saturating_sub(*lt);
    }
    last.insert(iface.clone(), (rx, tx));

    let mut guard = state.lock().await;
    let entry = guard.entry(iface).or_default();
    if entry.day_key != day_key {
        entry.day_key = day_key.clone();
        entry.day_rx = 0;
        entry.day_tx = 0;
    }
    if entry.month_key != month_key {
        entry.month_key = month_key.clone();
        entry.month_rx = 0;
        entry.month_tx = 0;
    }
    entry.rx_total += rx_delta;
    entry.tx_total += tx_delta;
    entry.day_rx += rx_delta;
    entry.day_tx += tx_delta;
    entry.month_rx += rx_delta;
    entry.month_tx += tx_delta;
}

/// 后台采集循环（默认 30 秒一次）。
pub async fn collector_loop(state: TrafficState) {
    let mut last: HashMap<String, (u64, u64)> = HashMap::new();
    loop {
        collect_once(&state, &mut last).await;
        sleep(Duration::from_secs(30)).await;
    }
}

/// 导出与 shell 版一致的 JSON。
pub async fn export_json(state: &TrafficState) -> Value {
    let guard = state.lock().await;
    let mut interfaces = Vec::new();
    for (name, s) in guard.iter() {
        let mut obj = Map::new();
        obj.insert("name".to_string(), Value::from(name.clone()));
        obj.insert(
            "traffic".to_string(),
            json!({
                "total": { "rx": s.rx_total, "tx": s.tx_total },
                "day": [{ "date": s.day_key, "rx": s.day_rx, "tx": s.day_tx }],
                "month": [{ "date": s.month_key, "rx": s.month_rx, "tx": s.month_tx }]
            }),
        );
        interfaces.push(Value::Object(obj));
    }
    json!({ "interfaces": interfaces })
}
