//! 写操作：带输入校验地把前端参数映射为 AT 命令，等价于原 luci-app 的
//! mt5700m-at 各类 advanced-set / lock / sms / pdp / sim-pin 等动作。

use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::at::ATClient;

pub type ActionResult = Result<String, String>;

async fn at(client: &Arc<ATClient>, cmd: &str) -> Result<String, String> {
    client
        .send_command(cmd.to_string())
        .await
        .map_err(|e| e.to_string())
}

// ========== 校验辅助 ==========

fn clean_csv(s: &str) -> String {
    let no_space: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let trimmed = no_space.trim_matches(',');
    // 折叠连续逗号
    let mut out = String::new();
    let mut prev_comma = false;
    for c in trimmed.chars() {
        if c == ',' {
            if !prev_comma {
                out.push(c);
            }
            prev_comma = true;
        } else {
            out.push(c);
            prev_comma = false;
        }
    }
    out.trim_matches(',').to_string()
}

fn csv_count(s: &str) -> usize {
    let v = clean_csv(s);
    if v.is_empty() {
        0
    } else {
        v.split(',').count()
    }
}

fn valid_numeric_csv(s: &str) -> bool {
    let v = clean_csv(s);
    !v.is_empty() && v.split(',').all(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
}

fn numeric_csv_in_range(s: &str, min: u64, max: u64) -> bool {
    if !valid_numeric_csv(s) {
        return false;
    }
    clean_csv(s).split(',').all(|f| {
        f.parse::<u64>()
            .map(|n| n >= min && n <= max)
            .unwrap_or(false)
    })
}

fn valid_lock_count(count: usize) -> bool {
    (1..=20).contains(&count)
}

fn safe_at_field(s: &str) -> bool {
    if s.chars().any(|c| c.is_control()) {
        return false;
    }
    !s.contains('"') && !s.contains(',') && !s.contains('\r') && !s.contains('\n')
}

fn valid_pin(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_digit())
        && (4..=8).contains(&s.len())
}

fn valid_puk(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit()) && s.len() == 8
}

fn err_usage(msg: &str) -> String {
    format!("参数校验失败: {}", msg)
}

// ========== 锁频命令构建 ==========

fn build_lte_lock_command(
    lock_type: u8,
    bands: &str,
    arfcns: &str,
    pcis: &str,
) -> Result<String, String> {
    let bands = clean_csv(bands);
    let arfcns = clean_csv(arfcns);
    let pcis = clean_csv(pcis);
    match lock_type {
        0 => Ok("AT^LTEFREQLOCK=0".to_string()),
        3 => {
            let count = csv_count(&bands);
            if valid_lock_count(count) && numeric_csv_in_range(&bands, 0, 65535) {
                Ok(format!("AT^LTEFREQLOCK=3,0,{},\"{}\"", count, bands))
            } else {
                Err(err_usage("LTE 频段锁"))
            }
        }
        1 => {
            let count = csv_count(&bands);
            if valid_lock_count(count)
                && numeric_csv_in_range(&bands, 0, 65535)
                && numeric_csv_in_range(&arfcns, 0, 4294967295)
                && count == csv_count(&arfcns)
            {
                Ok(format!(
                    "AT^LTEFREQLOCK=1,0,{},\"{}\",\"{}\"",
                    count, bands, arfcns
                ))
            } else {
                Err(err_usage("LTE 频段+频点锁"))
            }
        }
        2 => {
            let count = csv_count(&bands);
            if valid_lock_count(count)
                && numeric_csv_in_range(&bands, 0, 65535)
                && numeric_csv_in_range(&arfcns, 0, 4294967295)
                && numeric_csv_in_range(&pcis, 0, 503)
                && count == csv_count(&arfcns)
                && count == csv_count(&pcis)
            {
                Ok(format!(
                    "AT^LTEFREQLOCK=2,0,{},\"{}\",\"{}\",\"{}\"",
                    count, bands, arfcns, pcis
                ))
            } else {
                Err(err_usage("LTE 频段+频点+小区锁"))
            }
        }
        _ => Err(err_usage("LTE 锁类型")),
    }
}

fn build_nr_lock_command(
    lock_type: u8,
    bands: &str,
    arfcns: &str,
    scs: &str,
    pcis: &str,
) -> Result<String, String> {
    let bands = clean_csv(bands);
    let arfcns = clean_csv(arfcns);
    let scs = clean_csv(scs);
    let pcis = clean_csv(pcis);
    match lock_type {
        0 => Ok("AT^NRFREQLOCK=0".to_string()),
        3 => {
            let count = csv_count(&bands);
            if valid_lock_count(count) && numeric_csv_in_range(&bands, 0, 65535) {
                Ok(format!("AT^NRFREQLOCK=3,0,{},\"{}\"", count, bands))
            } else {
                Err(err_usage("NR 频段锁"))
            }
        }
        1 => {
            let count = csv_count(&bands);
            if valid_lock_count(count)
                && numeric_csv_in_range(&bands, 0, 65535)
                && numeric_csv_in_range(&arfcns, 0, 4294967295)
                && numeric_csv_in_range(&scs, 0, 4)
                && count == csv_count(&arfcns)
                && count == csv_count(&scs)
            {
                Ok(format!(
                    "AT^NRFREQLOCK=1,0,{},\"{}\",\"{}\",\"{}\"",
                    count, bands, arfcns, scs
                ))
            } else {
                Err(err_usage("NR 频段+频点锁"))
            }
        }
        2 => {
            let count = csv_count(&bands);
            if valid_lock_count(count)
                && numeric_csv_in_range(&bands, 0, 65535)
                && numeric_csv_in_range(&arfcns, 0, 4294967295)
                && numeric_csv_in_range(&scs, 0, 4)
                && numeric_csv_in_range(&pcis, 0, 1007)
                && count == csv_count(&arfcns)
                && count == csv_count(&scs)
                && count == csv_count(&pcis)
            {
                Ok(format!(
                    "AT^NRFREQLOCK=2,0,{},\"{}\",\"{}\",\"{}\",\"{}\"",
                    count, bands, arfcns, scs, pcis
                ))
            } else {
                Err(err_usage("NR 频段+频点+小区锁"))
            }
        }
        _ => Err(err_usage("NR 锁类型")),
    }
}

async fn apply_frequency_lock(
    client: &Arc<ATClient>,
    rat: &str,
    lock_type: u8,
    lock_cmd: &str,
) -> ActionResult {
    let (query, prefix) = match rat {
        "lte" => ("AT^LTEFREQLOCK?", "^LTEFREQLOCK"),
        "nr" => ("AT^NRFREQLOCK?", "^NRFREQLOCK"),
        _ => return Err(err_usage("制式 rat")),
    };

    let cfun_raw = at(client, "AT+CFUN?").await.unwrap_or_default();
    let previous = crate::parse::first_match(&cfun_raw, "+CFUN:")
        .map(|v| crate::parse::clean_value(&v))
        .unwrap_or_else(|| "1".to_string());
    let previous = match previous.as_str() {
        "0" | "1" => previous.clone(),
        _ => "1".to_string(),
    };

    at(client, lock_cmd).await?;

    // 频锁变更需重启无线功能后才生效（保持既有飞行模式会话离线）。
    if previous == "1" {
        let _ = at(client, "AT+CFUN=0").await;
        sleep(Duration::from_secs(1)).await;
        let _ = at(client, "AT+CFUN=1").await;
    }

    for _ in 0..8 {
        let raw = at(client, query).await.unwrap_or_default();
        let current = crate::parse::first_match(&raw, prefix)
            .map(|v| crate::parse::clean_value(&v))
            .unwrap_or_default()
            .split(',')
            .next()
            .unwrap_or("")
            .to_string();
        if current == lock_type.to_string() {
            return Ok(raw);
        }
        if previous == "0" {
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }
    Err("MT5700M 频锁校验失败".to_string())
}

// ========== 各类动作 ==========

pub async fn lock(
    client: &Arc<ATClient>,
    rat: &str,
    lock_type: u8,
    bands: &str,
    arfcns: &str,
    scs: &str,
    pcis: &str,
) -> ActionResult {
    let lock_cmd = match rat {
        "lte" => build_lte_lock_command(lock_type, bands, arfcns, pcis)?,
        "nr" => build_nr_lock_command(lock_type, bands, arfcns, scs, pcis)?,
        _ => return Err(err_usage("制式 rat")),
    };
    apply_frequency_lock(client, rat, lock_type, &lock_cmd).await
}

pub async fn unlock(client: &Arc<ATClient>) -> ActionResult {
    let lte = at(client, "AT^LTEFREQLOCK=0").await.unwrap_or_default();
    let nr = at(client, "AT^NRFREQLOCK=0").await.unwrap_or_default();
    Ok(format!("Unlock LTE:\n{}\nUnlock NR:\n{}", lte, nr))
}

pub async fn airplane(client: &Arc<ATClient>, state: &str) -> ActionResult {
    match state {
        "0" | "1" => at(client, &format!("AT+CFUN={}", state)).await,
        _ => Err(err_usage("飞行模式状态")),
    }
}

pub async fn restart(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^RESET").await
}

pub async fn radio_mode(client: &Arc<ATClient>, requested: &str) -> ActionResult {
    match requested {
        "02" | "03" | "08" | "0302" | "0803" | "080302" => {}
        _ => return Err(err_usage("无线制式")),
    }
    let raw = at(client, "AT^SYSCFGEX?").await?;
    let current = crate::parse::first_match(&raw, "^SYSCFGEX:").unwrap_or_default();
    let suffix = current.split_once(',').map(|(_, s)| s).unwrap_or("");
    if suffix.is_empty() || suffix == current {
        return Err("无法读取当前无线策略".to_string());
    }
    at(
        client,
        &format!("AT^SYSCFGEX=\"{}\",{},,", requested, suffix),
    )
    .await
}

pub async fn radio_policy(
    client: &Arc<ATClient>,
    acqorder: &str,
    band: &str,
    roam: &str,
    srvdomain: &str,
    lteband: &str,
) -> ActionResult {
    match acqorder {
        "02" | "03" | "08" | "0302" | "0803" | "080302" => {}
        _ => return Err(err_usage("优选顺序")),
    }
    if !band.is_empty() && !band.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(err_usage("频段掩码"));
    }
    if roam != "0" && roam != "1" {
        return Err(err_usage("漫游策略"));
    }
    if srvdomain != "1" && srvdomain != "2" {
        return Err(err_usage("业务域"));
    }
    if !lteband.is_empty() && !lteband.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(err_usage("LTE 频段"));
    }
    at(
        client,
        &format!(
            "AT^SYSCFGEX=\"{}\",{},{},{},{},,",
            acqorder, band, roam, srvdomain, lteband
        ),
    )
    .await
}

pub async fn set_5g_access_mode(client: &Arc<ATClient>, preset: &str) -> ActionResult {
    let values = match preset {
        "option2" => "1,0,1",
        "option3" => "0,1,0",
        "option23" => "1,1,1",
        _ => return Err(err_usage("5G 接入模式")),
    };
    let raw = at(client, "AT+CFUN?").await?;
    let previous = crate::parse::first_match(&raw, "+CFUN:")
        .map(|v| crate::parse::clean_value(&v))
        .unwrap_or_default();
    if previous != "0" && previous != "1" {
        return Err("未知模组功能等级，拒绝变更".to_string());
    }
    // C5GOPTION 必须在飞行模式下写入。
    let _ = at(client, "AT+CFUN=0").await;
    let result = at(client, &format!("AT^C5GOPTION={}", values)).await;
    let _ = at(client, &format!("AT+CFUN={}", previous)).await;
    result
}

pub async fn autodial(
    client: &Arc<ATClient>,
    enable: &str,
    dial_mode: &str,
    protocol: &str,
    apn: &str,
    username: &str,
    password: &str,
    auth_type: &str,
) -> ActionResult {
    if enable != "0" && enable != "1" {
        return Err(err_usage("自动拨号开关"));
    }
    if enable == "0" {
        return at(client, "AT^SETAUTODIAL=0").await;
    }
    if dial_mode != "0" && dial_mode != "1" && dial_mode != "2" {
        return Err(err_usage("拨号模式"));
    }
    if protocol != "IP" && protocol != "IPV6" && protocol != "IPV4V6" {
        return Err(err_usage("PDP 类型"));
    }
    if auth_type != "0" && auth_type != "1" && auth_type != "2" {
        return Err(err_usage("鉴权类型"));
    }
    if !safe_at_field(apn) || !safe_at_field(username) || !safe_at_field(password) {
        return Err(err_usage("APN/用户名/密码含非法字符"));
    }
    if apn.len() > 99 || username.len() > 31 || password.len() > 31 {
        return Err(err_usage("APN/用户名/密码长度"));
    }
    at(
        client,
        &format!(
            "AT^SETAUTODIAL={},{},\"{}\",\"{}\",\"{}\",\"{}\",{}",
            enable, dial_mode, protocol, apn, username, password, auth_type
        ),
    )
    .await
}

// 简化的数值参数动作集合。
async fn single_digit(
    client: &Arc<ATClient>,
    value: &str,
    allowed: &[&str],
    cmd: &str,
) -> ActionResult {
    if !allowed.contains(&value) {
        return Err(err_usage(cmd));
    }
    at(client, &format!("{}={}", cmd, value)).await
}

pub async fn nic_speed(client: &Arc<ATClient>, v: &str) -> ActionResult {
    single_digit(client, v, &["1", "2"], "AT^TDPCIELANCFG").await
}
pub async fn pcie_controller(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "0" | "1" => at(client, &format!("AT^TDPMCFG={},0,0,0", v)).await,
        _ => Err(err_usage("PCIe 控制器")),
    }
}
pub async fn led(client: &Arc<ATClient>, v: &str) -> ActionResult {
    single_digit(client, v, &["0", "1"], "AT^LEDSWITCH").await
}
pub async fn usb_mode(client: &Arc<ATClient>, v: &str) -> ActionResult {
    // SETMODE=7 (MBIM) 暂不支持。
    single_digit(
        client,
        v,
        &["0", "1", "2", "3", "4", "5", "6", "8"],
        "AT^SETMODE",
    )
    .await
}
pub async fn interface_mode(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "1" | "2" => at(client, &format!("AT^TDCFG=\"infcfg\",\"mode\",{}", v)).await,
        _ => Err(err_usage("接口模式")),
    }
}
pub async fn postroute(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "2" => at(client, "AT^TDCFG=\"infcfg\",\"PostRoute\",2").await,
        "1" => {
            let _ = at(client, "AT^TDCFG=\"infcfg\",\"PostRoute\",1").await;
            at(client, "AT^IPFILTERSWITCH=0").await
        }
        _ => Err(err_usage("路由策略")),
    }
}
pub async fn dmz(client: &Arc<ATClient>, v: &str) -> ActionResult {
    if v == "0" {
        return at(client, "AT^TDCFG=\"infcfg\",\"dmz\",\"0\"").await;
    }
    let octets: Vec<&str> = v.split('.').collect();
    if octets.len() != 4
        || !octets
            .iter()
            .all(|o| o.parse::<u8>().is_ok() && !o.is_empty())
    {
        return Err(err_usage("DMZ IP"));
    }
    at(client, &format!("AT^TDCFG=\"infcfg\",\"dmz\",\"{}\"", v)).await
}
pub async fn sim_hotplug(client: &Arc<ATClient>, v: &str) -> ActionResult {
    single_digit(client, v, &["0", "1"], "AT^TDSIMHP").await
}
pub async fn sim_activation(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "0" | "1" => at(client, &format!("AT^HVSST=1,{}", v)).await,
        _ => Err(err_usage("SIM 激活")),
    }
}
pub async fn sim_slot(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "0" => at(client, "AT^SCICHG=0,1").await,
        "1" => at(client, "AT^SCICHG=1,0").await,
        _ => Err(err_usage("SIM 槽位")),
    }
}
pub async fn thermal(client: &Arc<ATClient>, enabled: &str, interval: &str) -> ActionResult {
    if enabled != "0" && enabled != "1" {
        return Err(err_usage("温控开关"));
    }
    if !["1", "2", "3", "4", "5", "10", "15", "30", "60"].contains(&interval) {
        return Err(err_usage("温控间隔"));
    }
    at(
        client,
        &format!("AT^THERMAUTOFUN={},0,{}", enabled, interval),
    )
    .await
}
pub async fn thermal_log(client: &Arc<ATClient>, serial: &str, file: &str) -> ActionResult {
    if serial != "0" && serial != "1" {
        return Err(err_usage("温控日志串口开关"));
    }
    if file != "0" && file != "1" {
        return Err(err_usage("温控日志文件开关"));
    }
    at(
        client,
        &format!("AT^THERMLDLOGSW={},{}", serial, file),
    )
    .await
}
pub async fn thermal_thresholds(client: &Arc<ATClient>, vals: &[String]) -> ActionResult {
    if vals.len() != 9 || !vals.iter().all(|v| v.parse::<u32>().is_ok()) {
        return Err(err_usage("温控阈值参数"));
    }
    let n: Vec<u32> = vals.iter().map(|v| v.parse().unwrap()).collect();
    // 单调递增的区间边界校验（v2>v1, v4>v2, v6>v4, v8>v6 且回差单调）。
    if n.iter().any(|&x| x > 150)
        || n[1] <= n[0]
        || n[3] <= n[1]
        || n[5] <= n[3]
        || n[7] <= n[5]
        || n[2] >= n[1]
        || n[4] >= n[3]
        || n[6] >= n[5]
        || n[8] >= n[7]
    {
        return Err(err_usage("温控阈值单调性"));
    }
    let joined = vals
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(",");
    at(client, &format!("AT^THERMLDAUTOPARA={}", joined)).await
}
pub async fn carrier_aggregation(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "0" | "1" => at(client, &format!("AT^NRRCCAPCFG=3,{}", v)).await,
        _ => Err(err_usage("载波聚合开关")),
    }
}
pub async fn vonr(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "0" | "1" | "2" | "3" => at(client, &format!("AT^NRRCCAPCFG=2,{}", v)).await,
        _ => Err(err_usage("VoNR 开关")),
    }
}
pub async fn dss(client: &Arc<ATClient>, rate: &str, dmrs: &str) -> ActionResult {
    if rate != "0" && rate != "1" {
        return Err(err_usage("DSS 速率"));
    }
    if dmrs != "0" && dmrs != "1" {
        return Err(err_usage("DSS DMRS"));
    }
    at(client, &format!("AT^NRRCCAPCFG=5,{},{}", rate, dmrs)).await
}

// ========== PDP ==========

pub async fn pdp_set(client: &Arc<ATClient>, cid: &str, pdp_type: &str, apn: &str) -> ActionResult {
    if !(1..=11).contains(&cid.parse::<u32>().unwrap_or(0)) {
        return Err(err_usage("PDP CID"));
    }
    if pdp_type != "IP" && pdp_type != "IPV6" && pdp_type != "IPV4V6" {
        return Err(err_usage("PDP 类型"));
    }
    if !safe_at_field(apn) || apn.len() > 99 {
        return Err(err_usage("APN"));
    }
    at(client, &format!("AT+CGDCONT={},\"{}\",\"{}\"", cid, pdp_type, apn)).await
}
pub async fn pdp_remove(client: &Arc<ATClient>, cid: &str) -> ActionResult {
    if !(1..=11).contains(&cid.parse::<u32>().unwrap_or(0)) {
        return Err(err_usage("PDP CID"));
    }
    at(client, &format!("AT+CGDCONT={}", cid)).await
}
pub async fn pdp_state(client: &Arc<ATClient>, state: &str, cid: &str) -> ActionResult {
    if state != "0" && state != "1" {
        return Err(err_usage("PDP 状态"));
    }
    if !(1..=11).contains(&cid.parse::<u32>().unwrap_or(0)) {
        return Err(err_usage("PDP CID"));
    }
    at(client, &format!("AT+CGACT={},{}", state, cid)).await
}
pub async fn ndis(client: &Arc<ATClient>, v: &str) -> ActionResult {
    match v {
        "0" | "1" => at(client, &format!("AT^NDISDUP=1,{}", v)).await,
        _ => Err(err_usage("NDIS 会话")),
    }
}
pub async fn flow_clear(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^DSFLOWCLR").await
}

// ========== SIM PIN ==========

pub async fn sim_pin(
    client: &Arc<ATClient>,
    operation: &str,
    a: &str,
    b: &str,
) -> ActionResult {
    match operation {
        "verify" => {
            if !valid_pin(a) {
                return Err(err_usage("PIN"));
            }
            at(client, &format!("AT+CPIN=\"{}\"", a)).await
        }
        "enable" | "disable" => {
            if !valid_pin(a) {
                return Err(err_usage("PIN"));
            }
            let lock = if operation == "enable" { 1 } else { 0 };
            at(client, &format!("AT+CLCK=\"SC\",{},\"{}\"", lock, a)).await
        }
        "change" => {
            if !valid_pin(a) || !valid_pin(b) {
                return Err(err_usage("PIN"));
            }
            at(client, &format!("AT+CPWD=\"SC\",\"{}\",\"{}\"", a, b)).await
        }
        "unblock" => {
            if !valid_puk(a) || !valid_pin(b) {
                return Err(err_usage("PUK/PIN"));
            }
            at(client, &format!("AT+CPIN=\"{}\",\"{}\"", a, b)).await
        }
        _ => Err(err_usage("PIN 操作")),
    }
}

// ========== IMS / 复位 / IMEI / FOTA ==========

pub async fn sms_ims(client: &Arc<ATClient>, state: &str) -> ActionResult {
    if state != "0" && state != "1" {
        return Err(err_usage("IMS 开关"));
    }
    // 失败时也恢复注册，避免模组停留在飞行模式。
    let _ = at(client, "AT+CFUN=0").await;
    let r = match state {
        "1" => {
            let _ = at(
                client,
                "AT+CGDCONT=5,\"IPV4V6\",\"ims\",\"\",0,0,0,0,1,1,1,,,,,,0,,0,0,0,0",
            )
            .await;
            let _ = at(client, "AT+CEUS=0").await;
            at(client, "AT^IMSSWITCH=1,0,0").await
        }
        _ => {
            let _ = at(
                client,
                "AT+CGDCONT=5,\"IPV4V6\",\"\",\"\",0,0,0,0,1,1,1,,,,,,0,,0,0,0,0",
            )
            .await;
            let _ = at(client, "AT+CEUS=1").await;
            at(client, "AT^IMSSWITCH=0,0,0").await
        }
    };
    let _ = at(client, "AT+CFUN=1").await;
    r
}
pub async fn factory_reset(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT&F0").await
}
pub async fn set_imei(client: &Arc<ATClient>, imei: &str) -> ActionResult {
    if imei.len() != 15 || !imei.chars().all(|c| c.is_ascii_digit()) {
        return Err(err_usage("IMEI"));
    }
    at(client, &format!("AT^PHYNUM=IMEI,{}", imei)).await
}
pub async fn fota_init(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^FOTAMODE=0,1,0,1").await
}
pub async fn fota_state(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^FOTASTATE?").await
}
pub async fn fota_progress(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^FOTADLQ").await
}
pub async fn fota_download(client: &Arc<ATClient>, url: &str) -> ActionResult {
    let clean: String = url
        .chars()
        .filter(|c| !matches!(c, '\0' | '\r' | '\n' | '"'))
        .collect();
    if clean.is_empty() {
        return Err(err_usage("FOTA 下载地址"));
    }
    at(client, &format!("AT^FOTAOEMDL=\"{}\"", clean)).await
}
pub async fn fota_start(client: &Arc<ATClient>, url: &str) -> ActionResult {
    if !url.starts_with("http://") || url.contains(',') {
        return Err(err_usage("FOTA 地址"));
    }
    let mut url = url.to_string();
    if !url.ends_with('/') {
        url.push('/');
    }
    let _ = at(client, "ATE0").await;
    let _ = at(client, "AT^FOTAMODE=0,1,0,1").await;
    at(client, &format!("AT^FOTAOEMDL=\"{}\"", url)).await
}
pub async fn fota_resume(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^FOTADL=1").await
}
pub async fn fota_upgrade(client: &Arc<ATClient>) -> ActionResult {
    at(client, "AT^FWUP").await
}

// ========== SMS ==========

pub async fn sms_delete(client: &Arc<ATClient>, index: &str) -> ActionResult {
    if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) || index.len() > 6 {
        return Err(err_usage("短信索引"));
    }
    at(client, &format!("AT+CMGD={}", index)).await
}
pub async fn sms_clear(client: &Arc<ATClient>) -> ActionResult {
    let _ = at(client, "AT+CMGF=0").await;
    at(client, "AT+CMGD=1,4").await
}
pub async fn sms_set(client: &Arc<ATClient>, key: &str, value: &str) -> ActionResult {
    match key {
        "smsc" => {
            let smsc: String = value.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
            if smsc.is_empty() {
                return Err(err_usage("短消息中心号码"));
            }
            at(client, &format!("AT+CSCA=\"{}\"", smsc)).await
        }
        "storage" => {
            if value != "SM" && value != "ME" {
                return Err(err_usage("短信存储"));
            }
            at(
                client,
                &format!("AT+CPMS=\"{}\",\"{}\",\"{}\"", value, value, value),
            )
            .await
        }
        _ => Err(err_usage("短信设置项")),
    }
}
