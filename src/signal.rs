//! 信号采集：单后台生产者周期性查询 AT^HCSQ?，缓存最新快照供 API 读取。
//! 等价于原 luci-app 的 mt5700m-signal（one producer, many consumers）。

use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::at::ATClient;
use crate::parse;

pub type SignalCache = Arc<Mutex<Option<(u64, Map<String, Value>)>>>;

pub fn new_cache() -> SignalCache {
    Arc::new(Mutex::new(None))
}

/// 读取缓存中的信号快照（30 秒内有效）。
pub async fn read_cached(cache: &SignalCache) -> Option<Map<String, Value>> {
    let guard = cache.lock().await;
    guard.as_ref().map(|(ts, m)| {
        let now = now_secs();
        if now.saturating_sub(*ts) <= 30 {
            Some(m.clone())
        } else {
            None
        }
    })?
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 后台采集循环；返回后永不结束（由 tokio::spawn 调用）。
pub async fn collector_loop(client: Arc<ATClient>, cache: SignalCache) {
    let mut failures = 0u32;
    loop {
        let started = std::time::Instant::now();

        match client.send_command("AT^HCSQ?".to_string()).await {
            Ok(raw) => {
                let parsed = parse::parse_signal(&raw);
                if parsed.contains_key("sysmode") {
                    let mut guard = cache.lock().await;
                    *guard = Some((now_secs(), parsed));
                    failures = 0;
                }
            }
            Err(_) => {
                failures += 1;
            }
        }

        // 成功 5 秒一次；失败按 5 秒递进，最多 30 秒。
        let base = if failures == 0 { 5 } else { (failures as u64 * 5 + 5).min(30) };
        let elapsed = started.elapsed();
        let wait = base.saturating_sub(elapsed.as_secs()).max(5);
        sleep(Duration::from_secs(wait)).await;
    }
}
