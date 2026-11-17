//! 日志模块：输出到 stderr 与 syslog（通过 logger 命令），避免依赖额外的日志 crate。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static LOG_ENABLED: AtomicBool = AtomicBool::new(true);
static LOG_LEVEL: AtomicU8 = AtomicU8::new(1); // 0=DEBUG 1=INFO 2=WARN 3=ERROR

fn level_value(level: &str) -> u8 {
    match level.trim().to_ascii_uppercase().as_str() {
        "DEBUG" => 0,
        "INFO" => 1,
        "WARN" | "WARNING" => 2,
        "ERROR" => 3,
        _ => 1,
    }
}

pub fn init(enabled: bool, level: &str) {
    LOG_ENABLED.store(enabled, Ordering::Relaxed);
    LOG_LEVEL.store(level_value(level), Ordering::Relaxed);
}

fn emit(level: u8, tag: &str, msg: &str) {
    if !LOG_ENABLED.load(Ordering::Relaxed) || level < LOG_LEVEL.load(Ordering::Relaxed) {
        return;
    }
    let (prefix, syslog_tag) = match level {
        0 => ("[DEBUG]", "debug"),
        2 => ("[WARN]", "warn"),
        3 => ("[ERROR]", "err"),
        _ => ("[INFO]", "info"),
    };
    eprintln!("{} [{}] {}", prefix, tag, msg);

    // 同时写入 syslog（OpenWrt 上 logread 可查）。
    let _ = std::process::Command::new("logger")
        .args(["-t", tag, "-p", syslog_tag, "--", msg])
        .spawn();
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::logger::log_debug_fmt(&format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::logger::log_info_fmt(&format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::logger::log_warn_fmt(&format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::logger::log_error_fmt(&format!($($arg)*)) };
}

pub fn log_debug_fmt(msg: &str) {
    emit(0, "mt5700m-service", msg);
}
pub fn log_info_fmt(msg: &str) {
    emit(1, "mt5700m-service", msg);
}
pub fn log_warn_fmt(msg: &str) {
    emit(2, "mt5700m-service", msg);
}
pub fn log_error_fmt(msg: &str) {
    emit(3, "mt5700m-service", msg);
}
