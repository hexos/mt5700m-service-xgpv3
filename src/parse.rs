//! AT 响应解析器：把模组返回的文本解析成结构化键值，等价于原 luci-app 的
//! awk/sed 解析逻辑。返回 serde_json Map，便于直接序列化给前端。

use serde_json::{Map, Value};

/// 去掉首尾空白与首尾双引号。
pub fn clean_value(s: &str) -> String {
    let mut v = s.trim();
    if v.starts_with('"') {
        v = &v[1..];
    }
    if v.ends_with('"') {
        v = &v[..v.len() - 1];
    }
    v.trim().to_string()
}

/// 取第一行以 prefix 开头的行，去掉 prefix 与 \r。
pub fn first_match<'a>(raw: &'a str, prefix: &str) -> Option<String> {
    for line in raw.lines() {
        let l = line.trim_end_matches('\r');
        if let Some(rest) = l.strip_prefix(prefix) {
            return Some(rest.trim_start().to_string());
        }
    }
    None
}

/// 按逗号拆分并去掉每个字段空白与引号。
pub fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|f| f.trim_matches(|c: char| c == ' ' || c == '"').to_string())
        .collect()
}

fn is_valid_numeric(v: &str) -> bool {
    !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) && v != "255"
}

fn parse_i64(v: &str) -> Option<i64> {
    v.parse::<i64>().ok()
}

fn push_num(map: &mut Map<String, Value>, key: &str, v: i64) {
    map.insert(key.to_string(), Value::from(v));
}

fn push_float(map: &mut Map<String, Value>, key: &str, v: f64) {
    map.insert(key.to_string(), Value::from((v * 10.0).round() / 10.0));
}

/// 解析 `AT^HCSQ?` 信号：NR/LTE/WCDMA/GSM 各自的字段布局。
pub fn parse_signal(raw: &str) -> Map<String, Value> {
    let mut map = Map::new();
    let line = match first_match(raw, "^HCSQ:") {
        Some(l) => l,
        None => return map,
    };
    let fields = split_csv(&line);
    if fields.is_empty() {
        return map;
    }
    let sys = clean_value(&fields[0]);
    map.insert("sysmode".to_string(), Value::from(sys.clone()));

    // 与 shell 一致：值 255 无效。
    let rssi = |v: &str| -> Option<i64> {
        if is_valid_numeric(v) {
            parse_i64(v).map(|x| x - 121)
        } else {
            None
        }
    };
    let rsrp = |v: &str| -> Option<i64> {
        if is_valid_numeric(v) {
            parse_i64(v).map(|x| if x >= 97 { -44 } else { x - 141 })
        } else {
            None
        }
    };
    let rscp = |v: &str| -> Option<i64> {
        if is_valid_numeric(v) {
            parse_i64(v).map(|x| if x >= 96 { -25 } else { x - 121 })
        } else {
            None
        }
    };
    let sinr = |v: &str| -> Option<f64> {
        if is_valid_numeric(v) {
            parse_i64(v).map(|x| if x >= 251 { 30.0 } else { -20.2 + x as f64 * 0.2 })
        } else {
            None
        }
    };
    let rsrq = |v: &str| -> Option<f64> {
        if is_valid_numeric(v) {
            parse_i64(v).map(|x| if x >= 34 { -3.0 } else { -20.0 + x as f64 * 0.5 })
        } else {
            None
        }
    };

    match sys.as_str() {
        "NR" => {
            if let Some(v) = fields.get(1).and_then(|s| rsrp(s)) {
                push_num(&mut map, "rsrp", v);
            }
            if let Some(v) = fields.get(2).and_then(|s| sinr(s)) {
                push_float(&mut map, "sinr", v);
            }
            if let Some(v) = fields.get(3).and_then(|s| rsrq(s)) {
                push_float(&mut map, "rsrq", v);
            }
        }
        "LTE" => {
            if let Some(v) = fields.get(1).and_then(|s| rssi(s)) {
                push_num(&mut map, "rssi", v);
            }
            if let Some(v) = fields.get(2).and_then(|s| rsrp(s)) {
                push_num(&mut map, "rsrp", v);
            }
            if let Some(v) = fields.get(3).and_then(|s| sinr(s)) {
                push_float(&mut map, "sinr", v);
            }
            if let Some(v) = fields.get(4).and_then(|s| rsrq(s)) {
                push_float(&mut map, "rsrq", v);
            }
        }
        "WCDMA" => {
            if let Some(v) = fields.get(1).and_then(|s| rssi(s)) {
                push_num(&mut map, "rssi", v);
            }
            if let Some(v) = fields.get(2).and_then(|s| rscp(s)) {
                push_num(&mut map, "rscp", v);
            }
            if let Some(v) = fields.get(3).and_then(|s| is_valid_numeric(s).then(|| parse_i64(s)).flatten()) {
                push_float(&mut map, "ecio", -32.5 + v as f64 * 0.5);
            }
        }
        "GSM" => {
            if let Some(v) = fields.get(1).and_then(|s| rssi(s)) {
                push_num(&mut map, "rssi", v);
            }
        }
        _ => {}
    }
    map
}

/// 解析 `ATI` 设备身份信息。
pub fn parse_identity(raw: &str, imei_fallback: Option<&str>) -> Map<String, Value> {
    let mut map = Map::new();
    if let Some(v) = first_match(raw, "Manufacturer:") {
        map.insert("manufacturer".to_string(), Value::from(clean_value(&v)));
    }
    if let Some(v) = first_match(raw, "Model:") {
        map.insert("model".to_string(), Value::from(clean_value(&v)));
    }
    if let Some(v) = first_match(raw, "Revision:") {
        map.insert("revision".to_string(), Value::from(clean_value(&v)));
    }
    let mut imei = first_match(raw, "IMEI:").map(|v| clean_value(&v));
    if !matches!(imei.as_deref(), Some(i) if i.len() == 15 && i.chars().all(|c| c.is_ascii_digit()))
    {
        imei = imei_fallback
            .map(|s| s.to_string())
            .filter(|s| s.len() == 15 && s.chars().all(|c| c.is_ascii_digit()));
    }
    if let Some(i) = imei {
        map.insert("imei".to_string(), Value::from(i));
    }
    map.insert("product_name".to_string(), Value::from("MT5700M"));
    map
}

fn normalize_rat(v: &str) -> String {
    match v.trim() {
        "0" => "GSM",
        "2" => "UTRAN",
        "3" => "GSM EDGE",
        "4" => "HSDPA",
        "5" => "HSUPA",
        "6" => "HSDPA/HSUPA",
        "7" => "LTE",
        "9" => "NR",
        "10" => "LTE-M",
        "11" => "NB-IoT",
        "13" => "LTE",
        "20" => "NR",
        other => other,
    }
    .to_string()
}

/// 解析 SIM/运营商信息（CPIN + COPS + SYSINFOEX）。
pub fn parse_sim_operator(cpin: &str, cops: &str, sysinfoex: &str) -> Map<String, Value> {
    let mut map = Map::new();

    if let Some(v) = first_match(cpin, "+CPIN:") {
        map.insert("sim".to_string(), Value::from(clean_value(&v)));
    }

    if let Some(line) = first_match(cops, "+COPS:") {
        let fields = split_csv(&line);
        if let Some(name) = fields.get(2).filter(|s| !s.is_empty()) {
            map.insert("operator".to_string(), Value::from(name.clone()));
        }
        if let Some(rat) = fields.get(3) {
            map.insert("sysmode".to_string(), Value::from(normalize_rat(rat)));
        }
    }

    if let Some(line) = first_match(sysinfoex, "^SYSINFOEX:") {
        let fields = split_csv(&line);
        if let Some(mode) = fields.first() {
            map.insert("sysmode_detail".to_string(), Value::from(mode.clone()));
        }
    }
    map
}

/// 解析本机号码（AT+CNUM）。
pub fn parse_subscriber_number(raw: &str) -> Map<String, Value> {
    let mut map = Map::new();
    let mut number: Option<String> = None;
    for line in raw.lines() {
        if let Some(rest) = line.trim_end_matches('\r').strip_prefix("+CNUM:") {
            let fields = split_csv(rest);
            if let Some(v) = fields.get(1) {
                let cleaned: String = v.chars().filter(|c| !c.is_ascii_whitespace()).collect();
                if (cleaned.starts_with('+') || cleaned.chars().all(|c| c.is_ascii_digit()))
                    && cleaned.trim_start_matches('+').len() >= 5
                    && cleaned.trim_start_matches('+').chars().all(|c| c.is_ascii_digit())
                {
                    number = Some(cleaned);
                    break;
                }
            }
        }
    }
    match number {
        Some(n) => {
            map.insert("phone_number".to_string(), Value::from(n));
        }
        None => {
            if raw.contains("+CME ERROR: 22") {
                map.insert("phone_number_state".to_string(), Value::from("not_stored"));
            }
        }
    }
    map
}

/// 解析芯片温度（AT^CHIPTEMP?），12 个传感器取最高值为概览温度。
pub fn parse_temperature(raw: &str) -> Map<String, Value> {
    const NAMES: [&str; 12] = [
        "sub3g_pa",
        "sub6g_pa",
        "mimo_pa",
        "tcxo",
        "peri1",
        "peri2",
        "ap1",
        "ap2",
        "modem1",
        "modem2",
        "bbp1",
        "bbp2",
    ];
    let mut map = Map::new();
    let line = match first_match(raw, "^CHIPTEMP:") {
        Some(l) => l,
        None => return map,
    };
    let fields = split_csv(&line);
    let mut peak: Option<(i64, &str)> = None;
    for (i, name) in NAMES.iter().enumerate() {
        if let Some(f) = fields.get(i) {
            let v = clean_value(f);
            if let Ok(n) = v.parse::<i64>() {
                if !(-400..=1200).contains(&n) {
                    continue;
                }
                let temp = n as f64 / 10.0;
                push_float(&mut map, &format!("temp_{}", name), temp);
                if peak.map(|(p, _)| n > p).unwrap_or(true) {
                    peak = Some((n, name));
                }
            }
        }
    }
    if let Some((n, name)) = peak {
        push_float(&mut map, "temperature", n as f64 / 10.0);
        map.insert(
            "temperature_sensor".to_string(),
            Value::from(name.to_string()),
        );
    }
    map
}

/// 解析签约速率（AT^DSAMBR），返回 Kbps → Mbps。
pub fn parse_subscription_rate(raw: &str) -> Map<String, Value> {
    let mut map = Map::new();
    if let Some(line) = first_match(raw, "^DSAMBR:") {
        let fields = split_csv(&line);
        if fields.len() >= 3 {
            let down = clean_value(&fields[1]);
            let up = clean_value(&fields[2]);
            if let (Ok(d), Ok(u)) = (down.parse::<f64>(), up.parse::<f64>()) {
                push_float(&mut map, "ambr_down_mbps", d / 1000.0);
                push_float(&mut map, "ambr_up_mbps", u / 1000.0);
            }
        }
    }
    map
}

fn bandwidth(code: i64) -> f64 {
    match code {
        0 => 1.4,
        1 => 3.0,
        2 => 5.0,
        3 => 10.0,
        4 => 15.0,
        5 => 20.0,
        _ => 0.0,
    }
}

/// 解析载波聚合信息（HFREQINFO + CASCELLINFO + MONSSC）。
pub fn parse_carrier_aggregation(hfreq: &str, cascell: &str, monssc: &str) -> Map<String, Value> {
    let mut map = Map::new();
    let mut carriers: Vec<Value> = Vec::new();
    let mut nr_count = 0i64;
    let mut lte_count = 0i64;
    let mut lte_scell_count = 0i64;
    let mut secondary_count = 0i64;
    let mut dl_total = 0f64;
    let mut ul_total = 0f64;
    let mut dc = false;

    for line in hfreq.lines() {
        let Some(rest) = line.trim_end_matches('\r').strip_prefix("^HFREQINFO:") else {
            continue;
        };
        let fields = split_csv(rest);
        if fields.len() < 2 {
            continue;
        }
        let rat = fields[1].as_str();
        let (radio, divisor, limit) = match rat {
            "7" => ("NR", 1000.0, 4),
            "6" => ("LTE", 10.0, 1),
            _ => continue,
        };
        let mut parsed = 0;
        let mut i = 2;
        while i + 6 < fields.len() && parsed < limit {
            let seg = &fields[i..i + 7];
            let all_numeric = seg.iter().all(|f| f.parse::<i64>().is_ok());
            if !all_numeric {
                i += 7;
                continue;
            }
            let vals: Vec<i64> = seg.iter().map(|f| f.parse::<i64>().unwrap()).collect();
            let band = if radio == "NR" {
                format!("n{}", vals[0])
            } else {
                format!("B{}", vals[0])
            };
            let dl_frequency = vals[2] as f64 / divisor;
            let ul_frequency = vals[5] as f64 / divisor;
            let dl_bandwidth = vals[3] as f64 / 1000.0;
            let mut ul_bandwidth = vals[6] as f64 / 1000.0;

            let ul_valid = if radio == "NR" {
                !(vals[4] == 0 && vals[5] == 0)
            } else {
                true
            };
            if radio == "NR" && !ul_valid {
                ul_bandwidth = 0.0;
            }

            dl_total += dl_bandwidth;
            ul_total += ul_bandwidth;

            if radio == "NR" {
                nr_count += 1;
            } else {
                lte_count += 1;
            }

            let mut carrier = Map::new();
            carrier.insert("radio".to_string(), Value::from(radio));
            carrier.insert("band".to_string(), Value::from(band));
            carrier.insert("cell_id".to_string(), Value::from(vals[1]));
            carrier.insert("dl_frequency".to_string(), Value::from(round2(dl_frequency)));
            carrier.insert("dl_bandwidth".to_string(), Value::from(round1(dl_bandwidth)));
            carrier.insert("ul_cell_id".to_string(), Value::from(vals[4]));
            carrier.insert("ul_frequency".to_string(), Value::from(round2(ul_frequency)));
            carrier.insert("ul_bandwidth".to_string(), Value::from(round1(ul_bandwidth)));
            if radio == "NR" {
                carrier.insert("ul_valid".to_string(), Value::from(ul_valid));
            }
            carriers.push(Value::Object(carrier));

            parsed += 1;
            i += 7;
        }
    }

    for line in cascell.lines() {
        let Some(rest) = line.trim_end_matches('\r').strip_prefix("^CASCELLINFO:") else {
            continue;
        };
        let fields = split_csv(rest);
        if fields.len() < 12 {
            continue;
        }
        if !fields[..12].iter().all(|f| f.parse::<i64>().is_ok()) {
            continue;
        }
        let vals: Vec<i64> = fields[..12].iter().map(|f| f.parse::<i64>().unwrap()).collect();
        let ul_bandwidth = bandwidth(vals[10]);
        let dl_bandwidth = bandwidth(vals[11]);
        lte_count += 1;
        lte_scell_count += 1;
        dl_total += dl_bandwidth;
        ul_total += ul_bandwidth;

        let mut carrier = Map::new();
        carrier.insert("radio".to_string(), Value::from("LTE"));
        carrier.insert("band".to_string(), Value::from(format!("B{}", vals[5])));
        carrier.insert("cell_id".to_string(), Value::from(vals[7]));
        carrier.insert("dl_frequency".to_string(), Value::from(round2(vals[9] as f64 / 10.0)));
        carrier.insert("dl_bandwidth".to_string(), Value::from(round1(dl_bandwidth)));
        carrier.insert("ul_cell_id".to_string(), Value::from(vals[6]));
        carrier.insert("ul_frequency".to_string(), Value::from(round2(vals[8] as f64 / 10.0)));
        carrier.insert("ul_bandwidth".to_string(), Value::from(round1(ul_bandwidth)));
        carriers.push(Value::Object(carrier));
    }

    for line in monssc.lines() {
        let t = line.trim_end_matches('\r');
        if let Some(rest) = t.strip_prefix("^MONSSC:") {
            if rest.trim_start().starts_with("NR,") {
                dc = true;
                secondary_count += 1;
            }
        }
    }

    if carriers.is_empty() {
        return map;
    }

    if nr_count > 0 && lte_count > 0 {
        dc = true;
    }
    let ca = nr_count > 1 || lte_scell_count > 0;
    let mode = if dc {
        if ca {
            "EN-DC + CA"
        } else {
            "EN-DC"
        }
    } else if nr_count > 0 {
        if nr_count > 1 {
            "NR-CA"
        } else {
            "NR"
        }
    } else if lte_count > 1 {
        "LTE-CA"
    } else {
        "LTE"
    };

    map.insert("carriers".to_string(), Value::Array(carriers));
    map.insert("carrier_count".to_string(), Value::from(nr_count + lte_count));
    map.insert("ca_active".to_string(), Value::from(ca));
    map.insert("dc_active".to_string(), Value::from(dc));
    map.insert("nr_carrier_count".to_string(), Value::from(nr_count));
    map.insert("lte_carrier_count".to_string(), Value::from(lte_count));
    map.insert("lte_secondary_count".to_string(), Value::from(lte_scell_count));
    map.insert("secondary_connection_count".to_string(), Value::from(secondary_count));
    map.insert("ca_mode".to_string(), Value::from(mode));
    map.insert("ca_dl_bandwidth".to_string(), Value::from(round1(dl_total)));
    map.insert("ca_ul_bandwidth".to_string(), Value::from(round1(ul_total)));
    map
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// 解析锁频状态（AT^NRFREQLOCK? / AT^LTEFREQLOCK?）。
pub fn parse_lock_status(nr_raw: &str, lte_raw: &str) -> Map<String, Value> {
    let mut map = Map::new();
    if let Some(v) = first_match(nr_raw, "^NRFREQLOCK:") {
        map.insert("nr_lock".to_string(), Value::from(clean_value(&v)));
    }
    if let Some(v) = first_match(lte_raw, "^LTEFREQLOCK:") {
        map.insert("lte_lock".to_string(), Value::from(clean_value(&v)));
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nr_signal() {
        let raw = "^HCSQ: \"NR\",97,50,10\r\nOK";
        let m = parse_signal(raw);
        assert_eq!(m["sysmode"], "NR");
        assert_eq!(m["rsrp"], -44);
        assert_eq!(m["sinr"], -10.2);
        assert_eq!(m["rsrq"], -15.0);
    }

    #[test]
    fn ignores_invalid_signal_value() {
        let raw = "^HCSQ: \"NR\",255,255,255\r\nOK";
        let m = parse_signal(raw);
        assert!(m.get("rsrp").is_none());
    }

    #[test]
    fn parses_temperature_peak() {
        let raw = "^CHIPTEMP: 250,300,280,400,350,360,370,380,390,410,420,430\r\nOK";
        let m = parse_temperature(raw);
        assert_eq!(m["temperature"], 43.0);
        assert_eq!(m["temperature_sensor"], "bbp2");
    }

    #[test]
    fn parses_operator_and_rat() {
        let cops = "+COPS: 0,0,\"China Mobile\",7\r\nOK";
        let sys = "^SYSINFOEX: \"LTE_NR\",1,2\r\nOK";
        let m = parse_sim_operator("+CPIN: READY", cops, sys);
        assert_eq!(m["operator"], "China Mobile");
        assert_eq!(m["sysmode"], "LTE");
        assert_eq!(m["sysmode_detail"], "LTE_NR");
    }
}
