'use strict';
/* 概览页：hero + 信号 + 载波聚合 + 地址 + 流量 */

window.registerPage('status', function (container) {
  const { U, UI } = window;
  let ctx = null;

  function networkModeLabel(raw, data) {
    if (data.dc_active === true && /^EN-DC( \+ CA)?$/.test(data.ca_mode || '')) return '5G NSA';
    return raw === 'NR-5GC' ? '5G SA' : raw;
  }
  function signalRadioLabel(raw) {
    const labels = { NR: '5G (NR)', LTE: '4G (LTE)', WCDMA: '3G (WCDMA)', UMTS: '3G (UMTS)', GSM: '2G (GSM)' };
    const key = String(raw || '').trim().toUpperCase();
    return labels[key] || raw;
  }
  function usbLinkLabel(data) {
    const speed = Number(data.usb_speed_mbps);
    if (!(speed > 0) || !Number.isFinite(speed)) return '--';
    if (speed === 480) return '480 Mbps (USB 2.0)';
    return speed >= 1000 ? (speed / 1000) + ' Gbps' : speed + ' Mbps';
  }

  function signalCard(data) {
    const rsrp = parseFloat(data.rsrp), rsrq = parseFloat(data.rsrq), sinr = parseFloat(data.sinr);
    const q = U.signalQuality('rsrp', rsrp);
    const active = isNaN(rsrp) ? 0 : Math.max(1, Math.round(q.percentage / 100 * 14));
    const bars = [];
    for (let i = 0; i < 14; i++) bars.push(h('span', { class: 'signal-bar' + (i < active ? ' on' : ''), style: { height: (8 + i * 3) + 'px' } }));
    return h('section', { class: 'ui-card focus-card', dataset: { signal: '' } }, [
      h('div', { class: 'focus-head' }, [h('div', { class: 'focus-title' }, '信号'), h('span', { class: 'badge ' + q.cls }, q.label)]),
      h('div', { class: 'data-age', dataset: { signalTime: '' } }, '等待动态信号更新…'),
      h('div', { class: 'signal-value' }, [h('strong', {}, isNaN(rsrp) ? '--' : String(rsrp)), h('span', {}, 'RSRP · dBm')]),
      h('div', { class: 'signal-bars ' + q.cls }, bars),
      h('div', {}, [
        h('div', { class: 'mini-row' }, [h('span', {}, 'RSRQ'), h('strong', {}, isNaN(rsrq) ? '--' : rsrq + ' dB')]),
        h('div', { class: 'mini-row' }, [h('span', {}, 'SINR'), h('strong', {}, isNaN(sinr) ? '--' : sinr + ' dB')]),
        h('div', { class: 'mini-row' }, [h('span', {}, '制式'), h('strong', { title: data.sysmode || '--' }, signalRadioLabel(data.sysmode || '--'))])
      ])
    ]);
  }

  function carrierCard(data) {
    const carriers = Array.isArray(data.carriers) ? data.carriers : [];
    const count = carriers.length;
    const active = data.ca_active === true || data.dc_active === true;
    let headline = '--', badge = '不可用';
    if (count) {
      if (data.dc_active && data.ca_active) { headline = 'EN-DC + CA'; badge = '双连接与聚合'; }
      else if (data.dc_active) { headline = 'EN-DC'; badge = '双连接'; }
      else if (data.ca_active) { headline = count + 'CA'; badge = '载波聚合中'; }
      else if (count === 1) { headline = '单载波'; badge = '未聚合'; }
      else badge = '未知';
    }
    return h('section', { class: 'ui-card focus-card' }, [
      h('div', { class: 'focus-head' }, [h('div', { class: 'focus-title' }, '载波状态'), h('span', { class: 'badge' + (active ? ' active' : '') }, badge)]),
      h('div', { class: 'carrier-main' }, headline),
      h('div', { class: 'band-list' }, count ? carriers.map(c => h('span', { class: 'band' }, c.band || '--')) : h('span', { class: 'data-age' }, '当前无载波信息')),
      h('div', { class: 'carrier-stats' }, [
        h('div', { class: 'mini-row' }, [h('span', {}, '下行带宽'), h('strong', {}, data.ca_dl_bandwidth ? data.ca_dl_bandwidth + ' MHz' : '--')]),
        h('div', { class: 'mini-row' }, [h('span', {}, '上行带宽'), h('strong', {}, data.ca_ul_bandwidth ? data.ca_ul_bandwidth + ' MHz' : '--')])
      ]),
      count ? h('details', { class: 'carrier-detail' }, [h('summary', {}, '各载波详情'), h('div', {}, carriers.map((c, i) => h('div', {}, `载波 ${i + 1} · ${c.band} · ${c.radio} · ${c.dl_frequency} MHz · 下行 ${c.dl_bandwidth || '--'} MHz / 上行 ${c.ul_bandwidth || '--'} MHz`)))]) : null
    ]);
  }

  function addressCard(conn) {
    const active = conn.ipv4_up || conn.ipv6_up;
    return h('section', { class: 'ui-card focus-card' }, [
      h('div', { class: 'focus-head' }, [h('div', { class: 'focus-title' }, '蜂窝接口'), h('span', { class: 'badge' + (active ? ' active' : '') }, active ? '已连接' : '未连接')]),
      h('div', { class: 'ip-row' }, [h('div', { class: 'ip-head' }, [h('span', {}, 'IPv4'), h('span', { class: 'ip-state' + (conn.ipv4_up ? ' on' : '') }, conn.ipv4_up ? '已连接' : '未分配')]), h('div', { class: 'ip-value' }, conn.network || '--')]),
      h('div', { class: 'ip-row' }, [h('div', { class: 'ip-head' }, [h('span', {}, 'IPv6'), h('span', { class: 'ip-state' + (conn.ipv6_up ? ' on' : '') }, conn.ipv6_up ? '已连接' : '未分配')]), h('div', { class: 'ip-value' }, conn.ipv6_up ? (conn.interface6 || '--') : '--')]),
      h('div', { class: 'ip-meta' }, [h('span', {}, conn.mode || '--'), h('span', {}, 'metric ' + (conn.metric || '--'))])
    ]);
  }

  function trafficPanel(report, ifaceName) {
    const iface = ((report && report.interfaces) || []).filter(i => i.name === ifaceName)[0] || ((report && report.interfaces) || [])[0] || { traffic: {} };
    const t = iface.traffic || {};
    const total = t.total || {}, day = (t.day || [])[0] || {}, month = (t.month || [])[0] || {};
    function stat(label, rx, tx) {
      return h('div', { class: 'ui-card traffic-stat' }, [
        h('div', { class: 'traffic-label' }, label),
        h('div', { class: 'traffic-value' }, U.formatBytes((Number(rx) || 0) + (Number(tx) || 0))),
        h('div', { class: 'traffic-split' }, [h('span', {}, '下载 ' + U.formatBytes(rx)), h('span', {}, '上传 ' + U.formatBytes(tx))])
      ]);
    }
    return h('section', { class: 'ui-card', style: { padding: '18px', marginTop: '14px' } }, [
      h('div', { class: 'traffic-head' }, [h('h3', {}, '流量统计'), h('div', { class: 'traffic-updated' }, '接口 ' + (iface.name || '--'))]),
      h('div', { class: 'traffic-layout' }, [stat('今日', day.rx, day.tx), stat('本月', month.rx, month.tx), stat('累计', total.rx, total.tx)])
    ]);
  }

  function shortcut(title, desc, path) {
    return h('a', { class: 'ui-card shortcut', href: '#/' + path }, [h('div', {}, [h('strong', {}, title), h('span', {}, desc)]), h('b', {}, '›')]);
  }

  function startSignalWatch(root, base) {
    if (root.dataset.watching) return;
    root.dataset.watching = '1';
    function tick() {
      if (!root.isConnected) return;
      Api.call('signal').then(sig => {
        if (!root.isConnected) return;
        const old = root.querySelector('[data-signal]');
        const grid = root.querySelector('.focus-grid');
        if (old && grid) { grid.replaceChild(signalCard(Object.assign({}, base, sig)), old); }
        const t = root.querySelector('[data-signal-time]');
        if (t) t.textContent = '更新于刚刚';
      }).catch(() => {});
    }
    setInterval(() => { if (root.isConnected) tick(); }, 5000);
    tick();
  }

  function render(r) {
    const data = r[0], conn = r[1], traffic = r[2];
    const op = data.operator || '';
    const operator = /[A-Za-z0-9\u4e00-\u9fff]/.test(op) ? op : '';
    const reachable = data.connected === true;
    const connected = conn.connected === true;
    const rawMode = data.sysmode_detail || data.sysmode || '--';
    const root = h('div', {}, [
      h('div', { class: 'hero' }, [
        h('div', {}, [
          h('h2', {}, 'MT5700M 模组'),
          connected ? null : h('div', { class: 'hero-sub' }, !reachable ? '模组无响应，请检查连接。' : '模组在线，但移动数据未连接。'),
          h('div', { class: 'hero-meta' }, [
            h('span', {}, ['运营商', h('strong', {}, operator || '--')]),
            h('span', {}, ['网络制式', h('strong', { title: rawMode }, networkModeLabel(rawMode, data))]),
            h('span', {}, ['网络接口', h('strong', {}, conn.network || '--')]),
            h('span', { title: 'USB 协商速率' }, ['USB', h('strong', {}, usbLinkLabel(data))])
          ])
        ]),
        h('div', { class: 'hero-side' }, [
          h('div', { class: 'status-pill' + (connected ? ' online' : '') }, [h('span', { class: 'dot' }), connected ? '已连接' : reachable ? '模组在线' : '不可用']),
          h('button', { class: 'btn ghost', onClick: () => ctx && ctx.refresh() }, '刷新')
        ])
      ]),
      h('div', { class: 'focus-grid' }, [signalCard(data), carrierCard(data), addressCard(conn)]),
      trafficPanel(traffic, conn.network || ''),
      h('div', { class: 'shortcuts' }, [
        shortcut('移动数据', 'APN、拨号与会话计数', 'connection'),
        shortcut('无线与小区', '频段、小区与无线策略', 'network'),
        shortcut('模组与 SIM', '模组身份、SIM 信息与维护', 'system')
      ])
    ]);
    startSignalWatch(root, data);
    return root;
  }

  ctx = asyncPage(container, () => Promise.all([Api.call('status'), Api.call('connection-status'), Api.call('traffic')]), render);
});
