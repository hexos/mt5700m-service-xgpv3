//! SMS 发送：UTF-8 → UCS2 PDU 编码，支持多段（最多 10 段），
//! 使用 MT5700M AT 手册 9.14.5 的字面 `\r` 分隔格式，等价于原 C 传输的 sms 逻辑。

use std::sync::Arc;
use std::time::Duration;

use crate::at::ATClient;

const MAX_UNITS: usize = 670;

fn valid_number(number: &str) -> bool {
    let digits = number.strip_prefix('+').unwrap_or(number);
    !digits.is_empty()
        && digits.len() <= 20
        && digits.chars().all(|c| c.is_ascii_digit())
}

fn utf8_to_utf16(text: &str) -> Result<Vec<u16>, String> {
    let mut units = Vec::new();
    for ch in text.chars() {
        let cp = ch as u32;
        if cp > 0x10FFFF || (0xD800..=0xDFFF).contains(&cp) {
            return Err("文本含非法 Unicode 码点".to_string());
        }
        if cp > 0xFFFF {
            let v = cp - 0x10000;
            units.push((0xD800 + (v >> 10)) as u16);
            units.push((0xDC00 + (v & 0x3FF)) as u16);
        } else {
            units.push(cp as u16);
        }
    }
    if units.is_empty() {
        return Err("短信内容为空".to_string());
    }
    if units.len() > MAX_UNITS {
        return Err(format!("短信内容超过 {} 个 UTF-16 码元", MAX_UNITS));
    }
    Ok(units)
}

fn is_high_surrogate(u: u16) -> bool {
    (0xD800..=0xDBFF).contains(&u)
}

fn pdu_bytes(number: &str, text: &[u16], total: u8, part: u8, reference: u8) -> Vec<u8> {
    let international = number.starts_with('+');
    let digits: Vec<char> = if international {
        number[1..].chars().collect()
    } else {
        number.chars().collect()
    };

    let mut b = Vec::new();
    b.push(0); // SMSC 地址长度
    b.push(if total > 1 { 0x41 } else { 0x01 }); // SMS-SUBMIT (+UDH)
    b.push(0); // 消息参考
    b.push(digits.len() as u8); // 目标号码长度
    b.push(if international { 0x91 } else { 0x81 }); // 号码类型

    let mut i = 0;
    while i < digits.len() {
        let hi = digits[i] as u8 - b'0';
        let lo = if i + 1 < digits.len() {
            digits[i + 1] as u8 - b'0'
        } else {
            0xF
        };
        b.push(hi | (lo << 4));
        i += 2;
    }

    b.push(0); // PID
    b.push(8); // DCS = UCS2
    let udl = text.len() * 2 + if total > 1 { 6 } else { 0 };
    b.push(udl as u8); // 用户数据长度
    if total > 1 {
        // 拼接信息 UDH：IEI=0, len=3, ref, total, part
        b.push(5);
        b.push(0);
        b.push(3);
        b.push(reference);
        b.push(total);
        b.push(part);
    }
    for &u in text {
        b.push((u >> 8) as u8);
        b.push((u & 0xFF) as u8);
    }
    b
}

fn sms_frame(bytes: &[u8]) -> String {
    let hex: String = bytes.iter().map(|b| format!("{:02X}", b)).collect();
    format!("AT+CMGS={}\\r{}", bytes.len() - 1, hex)
}

fn random_ref() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 256) as u8
}

/// 发送短信（PDU 模式，支持多段 UCS2）。永不因超时自动重发。
pub async fn send_sms(client: &Arc<ATClient>, number: &str, text: &str) -> Result<String, String> {
    if !valid_number(number) {
        return Err("无效的接收号码".to_string());
    }
    let units = utf8_to_utf16(text)?;

    // 分段：单段 ≤70，多段每段 ≤67，且不能拆开代理对。
    let mut parts: Vec<Vec<u16>> = Vec::new();
    let mut pos = 0;
    let total_units = units.len();
    while pos < total_units {
        let limit = if total_units <= 70 { 70 } else { 67 };
        let remaining = total_units - pos;
        let mut len = remaining.min(limit);
        if pos + len < total_units && is_high_surrogate(units[pos + len - 1]) {
            len -= 1;
        }
        if parts.len() == 10 {
            return Err("短信超过十段，未提交".to_string());
        }
        parts.push(units[pos..pos + len].to_vec());
        pos += len;
    }

    let reference = random_ref();
    let total = parts.len() as u8;

    // PDU 模式
    client
        .send_command("AT+CMGF=0".to_string())
        .await
        .map_err(|e| e.to_string())?;

    let mut submitted = 0usize;
    for (idx, part) in parts.iter().enumerate() {
        let pdu = pdu_bytes(number, part, total, (idx + 1) as u8, reference);
        let frame = sms_frame(&pdu);
        let resp = client
            .send_command_with_timeout(frame, Duration::from_secs(120))
            .await
            .map_err(|e| {
                format!(
                    "短信提交未确认（第 {}/{} 段，已提交 {} 段）: {}",
                    idx + 1,
                    total,
                    submitted,
                    e
                )
            })?;
        if !resp.contains("+CMGS:") {
            return Err(format!(
                "短信提交未确认（第 {}/{} 段）: {}",
                idx + 1,
                total,
                resp.trim()
            ));
        }
        submitted += 1;
    }

    Ok(format!("短信已提交：{} 段", total))
}
