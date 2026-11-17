//! MT5700M-CN USB 识别：厂商 3466，产品 3301(normal)/3302(upgrade)/3303(dump)，
//! 以及网络接口 / PCUI AT 串口检测。逻辑等价于原 luci-app 的 usb.sh。

use std::fs;
use std::path::{Path, PathBuf};

const USB_VENDOR: &str = "3466";
const SYSFS_USB: &str = "/sys/bus/usb/devices";
const SYS_CLASS_NET: &str = "/sys/class/net";
const DEV_ROOT: &str = "/dev";

#[derive(Debug, Clone, PartialEq)]
pub enum UsbState {
    Normal,
    Upgrade,
    Dump,
    Unknown,
}

impl UsbState {
    pub fn as_str(&self) -> &'static str {
        match self {
            UsbState::Normal => "normal",
            UsbState::Upgrade => "upgrade",
            UsbState::Dump => "dump",
            UsbState::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UsbInfo {
    pub state: UsbState,
    pub product: String,
    pub slot: String,
}

fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
}

fn state_from_product(product: &str) -> UsbState {
    match product {
        "3301" => UsbState::Normal,
        "3302" => UsbState::Upgrade,
        "3303" => UsbState::Dump,
        _ => UsbState::Unknown,
    }
}

/// 扫描 USB 设备，优先返回 normal 状态的模组。
pub fn usb_info() -> Option<UsbInfo> {
    let dir = Path::new(SYSFS_USB);
    let mut first: Option<UsbInfo> = None;

    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let dev = entry.path();
        let vendor = read_trim(&dev.join("idVendor"))?;
        if vendor != USB_VENDOR {
            continue;
        }
        let product = read_trim(&dev.join("idProduct")).unwrap_or_default();
        let slot = entry.file_name().to_string_lossy().to_string();
        let state = state_from_product(&product);
        let info = UsbInfo {
            state: state.clone(),
            product,
            slot,
        };
        if state == UsbState::Normal {
            return Some(info);
        }
        if first.is_none() {
            first = Some(info);
        }
    }
    first
}

pub fn normal_slot() -> Option<String> {
    let info = usb_info()?;
    if info.state == UsbState::Normal {
        Some(info.slot)
    } else {
        None
    }
}

/// 找到 normal 模组对应的网络接口名。
pub fn netdev() -> Option<String> {
    let slot = normal_slot()?;
    let base = Path::new(SYSFS_USB).join(&slot);

    // 遍历 <slot>/<iface>/net/* 与 <slot>:<cfg>/net/*
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&base) {
        for e in entries.flatten() {
            roots.push(e.path());
        }
    }
    // 同时匹配 <slot>:* 目录
    if let Ok(entries) = fs::read_dir(Path::new(SYSFS_USB)) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{}:", slot)) {
                roots.push(e.path());
            }
        }
    }

    for root in roots {
        let net_dir = root.join("net");
        if let Ok(entries) = fs::read_dir(&net_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if Path::new(SYS_CLASS_NET).join(&name).is_dir() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// 判断某个 tty 设备是否属于 normal 模组的 PCUI AT 端口。
fn port_belongs_to_normal(tty: &str) -> bool {
    let name = tty.rsplit('/').next().unwrap_or(tty);
    let dev_link = Path::new("/sys/class/tty").join(name).join("device");
    let dev = match fs::canonicalize(&dev_link) {
        Ok(d) => d,
        Err(_) => return false,
    };

    // 向上查找 idVendor/idProduct
    let mut cur = dev.as_path();
    loop {
        if cur.join("idVendor").is_file() && cur.join("idProduct").is_file() {
            let vendor = read_trim(&cur.join("idVendor")).unwrap_or_default();
            let product = read_trim(&cur.join("idProduct")).unwrap_or_default();
            return format!("{}:{}", vendor, product) == "3466:3301";
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return false,
        }
    }
}

/// 判断 tty 接口是否为 PCUI（class/subclass/protocol = ff:06:12 或 interface 描述含 PCUI）。
fn port_is_pcui(tty: &str) -> bool {
    if !port_belongs_to_normal(tty) {
        return false;
    }
    let name = tty.rsplit('/').next().unwrap_or(tty);
    let dev_link = Path::new("/sys/class/tty").join(name).join("device");
    let dev = fs::canonicalize(&dev_link).ok();
    let Some(dev) = dev else { return false };

    let mut cur = dev.as_path();
    let mut iface_dir: Option<PathBuf> = None;
    loop {
        if cur.join("bInterfaceClass").is_file() || cur.join("interface").is_file() {
            iface_dir = Some(cur.to_path_buf());
            break;
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }

    let Some(dir) = iface_dir else { return false };

    let class = read_trim(&dir.join("bInterfaceClass")).unwrap_or_default();
    let subclass = read_trim(&dir.join("bInterfaceSubClass")).unwrap_or_default();
    let protocol = read_trim(&dir.join("bInterfaceProtocol")).unwrap_or_default();
    if format!("{}:{}:{}", class, subclass, protocol) == "ff:06:12" {
        return true;
    }

    let desc = fs::read_to_string(dir.join("interface")).unwrap_or_default();
    let lower = desc.to_ascii_lowercase();
    lower.contains("pcui") || (lower.contains("pc") && lower.contains("ui"))
}

/// 找到 normal 模组的 PCUI AT 端口（如 /dev/ttyUSBx）。
pub fn pcui_port() -> Option<String> {
    normal_slot()?;
    let entries = fs::read_dir(DEV_ROOT).ok()?;
    let mut ports: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("ttyUSB") {
                Some(format!("{}/{}", DEV_ROOT, name))
            } else {
                None
            }
        })
        .collect();
    ports.sort();
    ports.into_iter().find(|p| port_is_pcui(p))
}

/// 读取指定 USB 槽位的协商速率（Mbps 数字字符串）。
pub fn usb_speed(slot: &str) -> Option<String> {
    if slot.is_empty() || !slot.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
        return None;
    }
    let speed = read_trim(&Path::new(SYSFS_USB).join(slot).join("speed"))?;
    if speed.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Some(speed)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_state_mapping() {
        assert_eq!(state_from_product("3301"), UsbState::Normal);
        assert_eq!(state_from_product("3302"), UsbState::Upgrade);
        assert_eq!(state_from_product("3303"), UsbState::Dump);
        assert_eq!(state_from_product("abcd"), UsbState::Unknown);
    }
}
