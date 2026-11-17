'use strict';
/* 解析工具：移植自 luci-app-mt5700m 的纯函数解析逻辑。 */

/* 把 mt5700m-service 返回的 labeled_raw 数组重组为 "===== label:\nraw\n" 文本 */
function labeledToRaw(items) {
  if (!Array.isArray(items)) return '';
  return items.map(it => `===== ${it.label || ''}:\n${it.raw || ''}`).join('\n');
}

/* 提取 "===== label:" 段落的原始文本 */
function section(raw, label) {
  const marker = '===== ' + label + ':';
  let active = false;
  const out = [];
  String(raw || '').split(/\n/).forEach(line => {
    if (line.indexOf('===== ') === 0) { active = line.indexOf(marker) === 0; return; }
    if (active && line.trim() && line.trim() !== 'OK') out.push(line.trim());
  });
  return out.join('\n');
}

/* 取段落后，再按前缀取首行值 */
function pick(text, expression, fallback) {
  const m = String(text || '').match(expression);
  return m ? m[1] : fallback;
}

function csvValues(text, prefix) {
  const line = String(text || '').split(/\n/).filter(l => l.indexOf(prefix) === 0)[0] || '';
  return line.substring(prefix.length).replace(/^[ :]+/, '').replace(/"/g, '').split(',').map(v => v.trim());
}
const matchValues = csvValues;

function hexIPv4(value) {
  if (!/^[0-9a-f]{8}$/i.test(value || '')) return '';
  return [6, 4, 2, 0].map(o => parseInt(value.substr(o, 2), 16)).join('.');
}

function hexNumber(value) {
  value = String(value || '').replace(/^0x/i, '');
  if (!/^[0-9a-f]+$/i.test(value)) return 0;
  if (value.length <= 8) return parseInt(value, 16);
  return parseInt(value.slice(0, -8), 16) * 4294967296 + parseInt(value.slice(-8), 16);
}

function formatBytes(value) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let idx = 0;
  value = Math.max(0, Number(value) || 0);
  while (value >= 1024 && idx < units.length - 1) { value /= 1024; idx++; }
  return (idx ? value.toFixed(value >= 10 ? 1 : 2) : String(Math.round(value))) + ' ' + units[idx];
}

function formatDuration(seconds) {
  seconds = Math.max(0, Number(seconds) || 0);
  const d = Math.floor(seconds / 86400), hh = Math.floor(seconds % 86400 / 3600), mm = Math.floor(seconds % 3600 / 60);
  return (d ? d + '天 ' : '') + (hh ? hh + '时 ' : '') + mm + '分';
}

function formatRate(value) {
  value = Number(value) || 0;
  if (value >= 1000000000) return (value / 1000000000).toFixed(2) + ' Gbps';
  if (value >= 1000000) return (value / 1000000).toFixed(1) + ' Mbps';
  return value ? Math.round(value / 1000) + ' Kbps' : '--';
}

function cleanCsv(value) { return String(value || '').replace(/\s+/g, '').replace(/^,+|,+$/g, '').replace(/,+/g, ','); }
function validCsv(value) { return /^[0-9]+(,[0-9]+)*$/.test(value); }
function csvInRange(value, min, max) {
  return validCsv(value) && value.split(',').every(n => { const v = Number(n); return v >= min && v <= max; });
}

function age(stamp) {
  const s = Math.floor(Date.now() / 1000 - Number(stamp));
  return Number(stamp) > 0 && s >= 0 ? s : null;
}
function ageText(stamp) {
  const s = age(stamp);
  return s === null ? '采样时间未知' : s < 60 ? s + ' 秒前' : Math.floor(s / 60) + ' 分钟前';
}

/* 解析会话地址（advanced/session 组） */
function parseSession(raw) {
  const ndis = csvValues(section(raw, 'Data session'), '^NDISSTATQRY');
  const dhcp4 = csvValues(section(raw, 'IPv4 lease'), '^DHCP');
  const dhcp6 = csvValues(section(raw, 'IPv6 lease'), '^DHCPV6');
  const flow = csvValues(section(raw, 'Data flow'), '^DSFLOWQRY');
  const mtu = csvValues(section(raw, 'MTU'), '^CGMTU');
  const pdpAddress = csvValues(section(raw, 'PDP address'), '+CGPADDR');
  const capability = pick(section(raw, 'IP capability'), /\^IPV6CAP:\s*(\w+)/, '');
  const capNames = { '1': '仅 IPv4', '2': '仅 IPv6', '7': 'IPv4 / IPv6 · 同 APN', '0B': 'IPv4 / IPv6 · 不同 APN', '0b': 'IPv4 / IPv6 · 不同 APN' };
  const detailed = (section(raw, 'Detailed sessions') || '').split(/\n/).map(line => {
    const m = line.match(/^\^DCONNSTAT:\s*(\d+)(?:[,，]["“”]?([^,"“”]*)["“”]?[,，](\d+)[,，](\d+)[,，](\d+)(?:[,，](\d+))?)?/);
    return m ? { cid: m[1], apn: m[2] || '', ipv4: m[3] === '1', ipv6: m[4] === '1', type: m[5] || '', ethernet: m[6] === '1' } : null;
  }).filter(x => x && x.apn);
  return {
    ipv4Connected: ndis[0] === '1' && ndis[4] === 'IPV4',
    ipv6Connected: ndis[5] === '1' && ndis[8] === 'IPV6',
    ipv4Address: hexIPv4(dhcp4[0]) || pdpAddress[1] || '',
    ipv4Gateway: hexIPv4(dhcp4[2]),
    ipv4Dns: [hexIPv4(dhcp4[4]), hexIPv4(dhcp4[5])].filter(Boolean).join(' · '),
    ipv6Address: dhcp6[0] && dhcp6[0] !== '::' ? dhcp6[0] : '',
    ipv6Dns: [dhcp6[4], dhcp6[5]].filter(v => v && v !== '::').join(' · '),
    capability: capNames[capability] || capability,
    mtu: mtu[1] && mtu[1] !== '0' ? mtu[1] : '网络默认',
    currentDuration: hexNumber(flow[0]), currentTx: hexNumber(flow[1]), currentRx: hexNumber(flow[2]),
    totalDuration: hexNumber(flow[3]), totalTx: hexNumber(flow[4]), totalRx: hexNumber(flow[5]),
    maximumDown: dhcp4[6] || dhcp6[6], maximumUp: dhcp4[7] || dhcp6[7],
    detailed
  };
}

/* ---- 服务小区解析 ---- */
function parseServingCell(values) {
  const rat = String(values[0] || '').toUpperCase();
  const cell = { rat: values[0] || '', mcc: values[1] || '', mnc: values[2] || '', arfcn: '', scs: '', cellId: '', pci: '', tac: '', metrics: [] };
  if (rat.indexOf('NR') === 0) {
    cell.arfcn = values[3] || ''; cell.scs = values[4] || ''; cell.cellId = values[5] || ''; cell.pci = values[6] || ''; cell.tac = values[7] || '';
    cell.metrics = [ { label: 'RSRP', value: values[8] || '', unit: 'dBm' }, { label: 'RSRQ', value: values[9] || '', unit: 'dB' }, { label: 'SINR', value: values[10] || '', unit: 'dB' } ];
  } else if (rat.indexOf('LTE') === 0) {
    cell.arfcn = values[3] || ''; cell.cellId = values[4] || ''; cell.pci = values[5] || ''; cell.tac = values[6] || '';
    cell.metrics = [ { label: 'RSRP', value: values[7] || '', unit: 'dBm' }, { label: 'RSRQ', value: values[8] || '', unit: 'dB' }, { label: 'RSSI', value: values[9] || '', unit: 'dBm' } ];
  } else if (rat.indexOf('WCDMA') === 0) {
    cell.arfcn = values[3] || ''; cell.cellId = values[5] || ''; cell.tac = values[6] || '';
    cell.metrics = [ { label: 'RSCP', value: values[7] || '', unit: 'dBm' }, { label: 'RXLEV', value: values[8] || '', unit: 'dBm' }, { label: 'ECIO', value: values[9] || '', unit: 'dB' } ];
  }
  return cell;
}

/* ---- 邻区扫描解析 ---- */
function parseCellScan(raw) {
  const ratNames = { '1': 'UMTS', '2': 'LTE', '3': '5G NR' };
  const scsNames = ['15', '30', '60', '120', '240'];
  return String(raw || '').split(/\n/).filter(l => /^\^CELLSCAN:\s*/.test(l)).map(line => {
    const f = line.replace(/^\^CELLSCAN:\s*/, '').split(',').map(v => v.trim().replace(/^"|"$/g, ''));
    const frequency = Number(f[2]);
    const band = parseInt(f[4], 16);
    const rsrp = /^-?\d+$/.test(f[11] || '') ? f[11] : '';
    const rsrq = /^-?\d+$/.test(f[12] || '') ? (Number(f[12]) * 0.5).toFixed(1) : '';
    const sinr = /^-?\d+$/.test(f[13] || '') ? (Number(f[13]) * 0.5).toFixed(1) : '';
    return { rat: ratNames[f[0]] || f[0] || '--', plmn: f[1] || '--', frequency: isFinite(frequency) ? (frequency / 1000).toFixed(1) : '--', pci: f[3] || '--', band: isFinite(band) ? 'B' + band : '--', lac: f[5] ? '0x' + f[5].toUpperCase() : '--', scs: scsNames[Number(f[10])] || '--', rsrp: rsrp || '--', rsrq: rsrq || '--', sinr: sinr || '--' };
  });
}

/* ---- SMS PDU 解码 ---- */
function swapDigits(v) { let o = ''; for (let i = 0; i < v.length; i += 2) o += (v[i + 1] || '') + v[i]; return o.replace(/F$/i, ''); }
function decodeUcs2(hex) { let o = ''; for (let i = 0; i + 3 < hex.length; i += 4) o += String.fromCharCode(parseInt(hex.substring(i, i + 4), 16)); return o; }
function decodeGsm7(hex, septets, skipBits) {
  const bytes = []; for (let i = 0; i < hex.length; i += 2) bytes.push(parseInt(hex.substring(i, i + 2), 16));
  const table = '@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞ\u001bÆæßÉ !"#¤%&\'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà';
  let out = '', escape = false;
  for (let n = 0; n < septets; n++) {
    const bit = (skipBits || 0) + n * 7, pos = bit >> 3, shift = bit & 7;
    const v = (((bytes[pos] || 0) >> shift) | ((bytes[pos + 1] || 0) << (8 - shift))) & 0x7f;
    if (escape) { out += ({ 10: '\f', 20: '^', 40: '{', 41: '}', 47: '\\', 60: '[', 61: '~', 62: ']', 64: '|', 101: '€' })[v] || ''; escape = false; }
    else if (v === 27) escape = true;
    else out += table[v] || ' ';
  }
  return out;
}
function decodePdu(pdu, index) {
  try {
    if (!/^(?:[0-9a-f]{2})+$/i.test(pdu)) return null;
    let p = 0; const smscLen = parseInt(pdu.substring(p, p + 2), 16); p += 2 + smscLen * 2;
    const first = parseInt(pdu.substring(p, p + 2), 16); p += 2;
    if (!Number.isFinite(first) || (first & 3) !== 0) return null;
    const digits = parseInt(pdu.substring(p, p + 2), 16); p += 2;
    const toa = parseInt(pdu.substring(p, p + 2), 16); p += 2;
    const numberHex = pdu.substring(p, p + Math.ceil(digits / 2) * 2); p += Math.ceil(digits / 2) * 2;
    let number = (toa === 145 ? '+' : '') + swapDigits(numberHex).substring(0, digits);
    if ((toa & 0x70) === 0x50) number = decodeGsm7(numberHex, Math.floor(digits * 4 / 7), 0);
    p += 2; const dcs = parseInt(pdu.substring(p, p + 2), 16); p += 2;
    const stamp = []; for (let s = 0; s < 6; s++, p += 2) stamp.push(swapDigits(pdu.substring(p, p + 2)));
    p += 2;
    const date = `20${stamp[0]}-${stamp[1]}-${stamp[2]} ${stamp[3]}:${stamp[4]}`;
    const udl = parseInt(pdu.substring(p, p + 2), 16); p += 2;
    const ud = pdu.substring(p);
    let headerBytes = 0, concat = null;
    if (!Number.isFinite(udl) || ud.length < ((dcs & 0x0c) === 0 ? Math.ceil(udl * 7 / 8) : udl) * 2) return null;
    if ((dcs & 0x0c) !== 0 && (dcs & 0x0c) !== 8) return null;
    if (first & 0x40) {
      headerBytes = parseInt(ud.substring(0, 2), 16) + 1;
      if (!Number.isFinite(headerBytes) || headerBytes * 2 > ud.length) return null;
      let hh = 2;
      while (hh < headerBytes * 2) {
        const iei = parseInt(ud.substring(hh, hh + 2), 16), len = parseInt(ud.substring(hh + 2, hh + 4), 16), value = ud.substring(hh + 4, hh + 4 + len * 2);
        hh += 4 + len * 2;
        if (!Number.isFinite(len) || hh > headerBytes * 2) return null;
        if (iei === 0 && len === 3) concat = { ref: parseInt(value.substring(0, 2), 16), total: parseInt(value.substring(2, 4), 16), seq: parseInt(value.substring(4, 6), 16) };
        if (iei === 8 && len === 4) concat = { ref: parseInt(value.substring(0, 4), 16), total: parseInt(value.substring(4, 6), 16), seq: parseInt(value.substring(6, 8), 16) };
      }
    }
    let text;
    if ((dcs & 0x0c) === 0x08) text = decodeUcs2(ud.substring(headerBytes * 2, udl * 2));
    else { const hs = Math.ceil(headerBytes * 8 / 7), skip = headerBytes ? hs * 7 : 0; text = decodeGsm7(ud, Math.max(0, udl - hs), skip); }
    return { index: String(index), indexes: [String(index)], number, date, text, concat, direction: 'in', order: Number(index) || 0 };
  } catch (e) { return null; }
}
function parseMessages(raw) {
  const lines = String(raw || '').replace(/\r/g, '').split('\n');
  const messages = []; let pending = null;
  lines.forEach(line => {
    line = line.trim();
    const m = line.match(/^\+CMGL:\s*(\d+),/);
    if (m) pending = m[1];
    else if (pending != null && /^[0-9A-F]+$/i.test(line)) { const msg = decodePdu(line, pending); if (msg) messages.push(msg); pending = null; }
  });
  let merged = []; const groups = {};
  messages.forEach(msg => {
    if (!msg.concat) { merged.push(msg); return; }
    const key = msg.number + ':' + msg.concat.ref + ':' + msg.concat.total + ':' + msg.date.substring(0, 10);
    (groups[key] || (groups[key] = [])).push(msg);
  });
  Object.keys(groups).forEach(key => {
    const parts = groups[key].sort((a, b) => a.concat.seq - b.concat.seq), first = parts[0];
    if (parts.length !== first.concat.total || !parts.every((p, i) => p.concat.seq === i + 1)) { merged = merged.concat(parts); return; }
    first.text = parts.map(p => p.text).join(''); first.indexes = parts.map(p => p.index); merged.push(first);
  });
  return merged.sort((a, b) => b.indexes[0] - a.indexes[0]);
}
function parseInfo(raw) {
  return {
    ims: ((raw.match(/\^IMSSWITCH:\s*(\d+)/) || [])[1] || ''),
    smsc: ((raw.match(/\+CSCA:\s*"([^"]+)"/) || [])[1] || ''),
    storage: ((raw.match(/\+CPMS:\s*"([A-Z]+)",(\d+),(\d+)/) || []).slice(1))
  };
}
function groupMessages(messages) {
  const groups = {};
  messages.forEach(m => { (groups[m.number] || (groups[m.number] = [])).push(m); });
  return Object.keys(groups).map(number => ({ number, messages: groups[number].sort((a, b) => (a.order || 0) - (b.order || 0)) }))
    .sort((a, b) => (b.messages[b.messages.length - 1].order || 0) - (a.messages[a.messages.length - 1].order || 0));
}

/* 信号质量分级 */
function signalQuality(kind, value) {
  if (isNaN(value)) return { label: '无数据', cls: 'unknown', percentage: 0 };
  let percentage, levels;
  if (kind === 'rsrp') { percentage = (value + 120) * 2.5; levels = [-80, -90, -100]; }
  else if (kind === 'rsrq') { percentage = (value + 25) * 4; levels = [-10, -15, -20]; }
  else { percentage = (value + 10) * 2.5; levels = [20, 13, 0]; }
  const idx = value >= levels[0] ? 0 : value >= levels[1] ? 1 : value >= levels[2] ? 2 : 3;
  return { label: ['极佳', '良好', '一般', '较弱'][idx], cls: ['excellent', 'good', 'fair', 'weak'][idx], percentage: Math.max(0, Math.min(100, percentage)) };
}

window.U = { labeledToRaw, section, pick, csvValues, matchValues, hexIPv4, hexNumber, formatBytes, formatDuration, formatRate, cleanCsv, validCsv, csvInRange, age, ageText, parseSession, parseServingCell, parseCellScan, decodePdu, parseMessages, parseInfo, groupMessages, signalQuality };
