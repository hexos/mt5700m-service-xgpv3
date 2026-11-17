'use strict';
/* 移动数据页：连接管理 + 会话 + 拨号策略 + PDP + 保存配置 + 日志 */

window.registerPage('connection', function (container) {
  const { U, UI } = window;
  let ctx = null;
  let loaded = { conn: {}, config: {}, connSettings: '', connRaw: '', sessionRaw: '' };

  function parseContexts(raw, activationRaw) {
    const active = {};
    String(activationRaw || '').split(/\n/).forEach(line => {
      const m = line.match(/^\+CGACT:\s*(\d+),(\d+)/);
      if (m) active[m[1]] = m[2] === '1';
    });
    return String(raw || '').split(/\n/).map(line => {
      const m = line.match(/^\+CGDCONT:\s*(\d+),"([^"]*)","([^"]*)"/);
      return m ? { cid: m[1], type: m[2], apn: m[3], active: active[m[1]] === true } : null;
    }).filter(Boolean);
  }

  function sessionPanel(raw) {
    const s = U.parseSession(raw);
    const active = s.ipv4Connected || s.ipv6Connected;
    const row = (label, value) => h('div', { class: 'kv-row' }, [h('span', {}, label), h('strong', {}, value || '--')]);
    const rows = [
      row('IPv4 地址', s.ipv4Address), row('IPv4 网关', s.ipv4Gateway), row('IPv4 DNS', s.ipv4Dns),
      row('IPv6 地址', s.ipv6Address), row('IPv6 DNS', s.ipv6Dns), row('IP 能力', s.capability), row('MTU', s.mtu)
    ];
    (s.detailed || []).forEach(d => rows.push(row('CID ' + d.cid + ' · ' + (d.apn || '运营商默认'), [d.ipv4 ? 'IPv4' : '', d.ipv6 ? 'IPv6' : '', d.ethernet ? 'Ethernet' : ''].filter(Boolean).join(' · '))));
    return h('div', { class: 'card-grid', style: { marginBottom: '14px' } }, [
      h('section', { class: 'ui-card control-card' }, [
        h('div', { style: { display: 'flex', justifyContent: 'space-between', alignItems: 'center' } }, [
          h('h3', {}, '模组会话地址'), h('span', { class: 'badge' + (active ? ' active' : '') }, active ? '已连接' : '未连接')
        ]),
        h('div', { style: { marginTop: '6px' } }, rows)
      ]),
      h('section', { class: 'ui-card control-card' }, [
        h('div', { style: { display: 'flex', justifyContent: 'space-between', alignItems: 'center' } }, [
          h('h3', {}, '模组流量计数'), h('span', { class: 'badge' }, '模组')
        ]),
        h('div', { style: { marginTop: '6px' } }, [
          row('本次时长', U.formatDuration(s.currentDuration)), row('本次总量', U.formatBytes(s.currentRx + s.currentTx)),
          row('累计时长', U.formatDuration(s.totalDuration)), row('累计总量', U.formatBytes(s.totalRx + s.totalTx)),
          row('网络最大下行', U.formatRate(s.maximumDown)), row('网络最大上行', U.formatRate(s.maximumUp))
        ]),
        UI.actionBtn('清除计数器', () => UI.confirmRun(() => Api.call('flow-clear'), '清除模组流量计数', '此操作将永久清除当前与累计数据流计数。', { danger: true }), 'default')
      ])
    ]);
  }

  function pdpPanel(raw) {
    const contexts = parseContexts(U.section(raw, 'PDP contexts'), U.section(raw, 'PDP activation'));
    function editPdp(c) {
      const cid = h('input', { class: 'input', value: c ? c.cid : '1' });
      const type = UI.select([['IPV4V6', 'IPv4 / IPv6'], ['IP', 'IPv4'], ['IPV6', 'IPv6']], c ? c.type : 'IPV4V6');
      const apn = h('input', { class: 'input', placeholder: '运营商默认', value: c ? c.apn : '' });
      const m = UI.modal(c ? '编辑 PDP 上下文' : '添加 PDP 上下文', h('div', {}, [
        UI.row('CID', cid), UI.row('IP 协议', type), UI.row('APN', apn)
      ]), [
        h('button', { class: 'btn default', onClick: m.close }, '取消'),
        h('button', { class: 'btn primary', onClick: async () => {
          const cidv = String(cid.value || '');
          if (!/^(?:[1-9]|1[01])$/.test(cidv) || /[",\r\n]/.test(apn.value || '')) { UI.toast('请输入 1–11 的 CID 和有效 APN', 'warning'); return; }
          m.close();
          await UI.confirmRun(() => Api.call('pdp-set', { cid: cidv, pdp_type: type.value, apn: apn.value.trim() }), '保存 PDP 上下文', '此操作会修改模组原生 PDP 配置。');
          ctx && ctx.refresh();
        } }, '保存')
      ]);
    }
    return h('section', { class: 'ui-card control-card', style: { marginBottom: '14px' } }, [
      h('div', { style: { display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' } }, [
        h('div', {}, [h('h3', {}, '模组 PDP 上下文'), h('div', { class: 'control-desc' }, '高级模组原生配置。IMS 上下文受保护；普通用户请在上方拨号配置里设置 APN。')]),
        h('button', { class: 'btn primary', onClick: () => editPdp(null) }, '添加配置')
      ]),
      contexts.length ? contexts.map(c => {
        const reserved = c.cid === '0' || c.cid === '5' || c.cid === '6' || String(c.apn || '').toLowerCase() === 'ims';
        return h('div', { class: 'control-row', style: { gridTemplateColumns: '60px 100px 1fr 90px auto' } }, [
          h('strong', {}, 'CID ' + c.cid), h('span', {}, c.type), h('span', {}, c.apn || '运营商默认'),
          h('span', { class: 'badge' + (c.active ? ' active' : '') }, c.active ? '激活' : '未激活'),
          reserved ? h('span', { class: 'control-note' }, String(c.apn || '').toLowerCase() === 'ims' ? 'IMS 保留' : '系统保留') : h('div', { style: { display: 'flex', gap: '6px' } }, [
            h('button', { class: 'btn', onClick: () => editPdp(c) }, '编辑'),
            h('button', { class: 'btn', onClick: () => UI.confirmRun(() => Api.call('pdp-state', { state: c.active ? '0' : '1', cid: c.cid }), c.active ? '停用 PDP' : '激活 PDP', '更改 PDP 上下文可能中断蜂窝服务。').then(() => ctx && ctx.refresh()) }, c.active ? '停用' : '激活'),
            h('button', { class: 'btn danger', onClick: () => UI.confirmRun(() => Api.call('pdp-remove', { cid: c.cid }), '删除 PDP 上下文', '确认从模组删除 CID ' + c.cid + '？', { danger: true }).then(() => ctx && ctx.refresh()) }, '删除')
          ])
        ]);
      }) : h('div', { class: 'control-note' }, '未读取到 PDP 上下文。')
    ]);
  }

  function moduleTools(raw) {
    const autoRaw = U.section(raw, 'Auto dial');
    const interfaceRaw = U.section(raw, 'Interface mode');
    const autoMatch = autoRaw.match(/\^SETAUTODIAL:\s*(\d+),(\d+),"([^"]*)",?"?([^",]*)"?,?"?([^",]*)"?,?"?([^",]*)"?,(\d+)/);
    const enabled = UI.select([['1', '启用'], ['0', '禁用']], autoMatch ? autoMatch[1] : '1');
    const dialMode = UI.select([['0', '模组内部拨号'], ['1', 'USB 主机拨号'], ['2', '以太网主机拨号']], autoMatch ? autoMatch[2] : '1');
    const protocol = UI.select([['IPV4V6', 'IPv4 / IPv6'], ['IP', 'IPv4'], ['IPV6', 'IPv6']], autoMatch ? autoMatch[3] : 'IPV4V6');
    const apn = h('input', { class: 'input', placeholder: '留空使用运营商默认', value: autoMatch ? autoMatch[4] : '' });
    const user = h('input', { class: 'input', value: autoMatch ? autoMatch[5] : '' });
    const pass = h('input', { class: 'input', type: 'password', value: autoMatch ? autoMatch[6] : '' });
    const auth = UI.select([['0', '无'], ['1', 'PAP'], ['2', 'CHAP']], autoMatch ? autoMatch[7] : '0');
    const postRouteValue = U.pick(interfaceRaw, /PostRoute:\s*(\d+)/, '');
    const dmzValue = U.pick(interfaceRaw, /Dmz:\s*([^\n]+)/, '').trim();
    const postRoute = UI.select([['2', '禁用'], ['1', '启用']], postRouteValue === '1' || postRouteValue === '2' ? postRouteValue : '2');
    const dmz = h('input', { class: 'input', placeholder: '192.168.8.100', value: dmzValue.indexOf('not cfg') < 0 ? dmzValue : '' });

    return h('div', { class: 'control-grid' }, [
      UI.card('模组拨号策略', '选择 MT5700M 固件如何暴露移动数据会话。', [
        h('div', { class: 'control-note' }, '每个数据路径使用一个拨号属主；集成 OpenWrt 服务时保持 USB 主机拨号，避免反复重连。'),
        UI.row('自动拨号', enabled), UI.row('拨号方式', dialMode), UI.row('PDP 协议', protocol),
        UI.row('APN', apn), UI.row('用户名', user), UI.row('密码', pass), UI.row('认证', auth),
        UI.actionBtn('应用模组拨号', () => UI.confirmRun(() => Api.call('autodial', { enable: enabled.value, dial_mode: dialMode.value, protocol: protocol.value, apn: apn.value, username: user.value, password: pass.value, auth_type: auth.value }), '应用拨号设置', '移动数据会话可能断开并重连。', { danger: true }))
      ]),
      UI.card('入站路由', 'MT5700M 数据路径后侧设备的可选模组侧转发。', [
        UI.row('后路由', postRoute),
        h('div', { class: 'control-note' }, '后路由与 DMZ 互斥；除非模组本身提供下游局域网，否则保持关闭。'),
        UI.actionBtn('应用后路由', () => UI.confirmRun(() => Api.call('postroute', { value: postRoute.value }), '更改后路由', '新路由路径需模组重启或切换飞行模式后生效。', { danger: true })),
        UI.row('DMZ 地址', dmz),
        UI.actionBtn('应用 DMZ', () => UI.confirmRun(() => Api.call('dmz', { value: dmz.value.trim() || '0' }), '更改 DMZ 主机', '所选 IPv4 主机可能暴露于移动网络的非请求流量。', { danger: true }))
      ])
    ]);
  }

  function configForm(config) {
    const c = config.connection || {};
    const apn = h('input', { class: 'input', placeholder: '自动', value: c.apn || '' });
    const pdp = UI.select([['ip', 'IPv4'], ['ipv6', 'IPv6'], ['ipv4v6', 'IPv4 / IPv6']], c.pdp_type || 'ipv4v6');
    const auth = UI.select([['none', '无'], ['pap', 'PAP'], ['chap', 'CHAP']], c.auth || 'none');
    const user = h('input', { class: 'input', value: c.username || '' });
    const pass = h('input', { class: 'input', type: 'password', value: c.password || '' });
    const metric = h('input', { class: 'input', type: 'number', value: c.metric || '50' });
    return h('div', { class: 'control-body' }, [
      UI.row('APN', apn), UI.row('IP 协议', pdp), UI.row('认证', auth), UI.row('用户名', user), UI.row('密码', pass), UI.row('路由度量', metric),
      UI.actionBtn('保存连接设置', () => UI.confirmRun(async () => {
        const sets = [
          ['apn', apn.value.trim()], ['pdp_type', pdp.value], ['auth', auth.value], ['username', user.value.trim()], ['password', pass.value], ['metric', metric.value]
        ];
        for (const [k, v] of sets) await Api.call('config-set', { section: 'connection', key: k, value: v });
      }, '保存连接设置', '保存后需重拨才能使用新设置。'))
    ]);
  }

  function render() {
    const conn = loaded.conn, config = loaded.config;
    const online = conn.connected === true;
    const c = config.connection || {};
    const delegated = String(c.managed || '').toLowerCase() === 'qmodem';
    const protocolNames = { ip: 'IPv4', ipv6: 'IPv6', ipv4v6: 'IPv4 / IPv6' };
    const root = h('div', {}, [
      h('div', { class: 'hero' }, [
        h('div', {}, [h('h2', {}, '移动数据'), h('div', { class: 'hero-sub' }, '配置 MT5700M 如何连接移动网络。')]),
        h('div', { class: 'status-pill' + (online ? ' online' : '') }, [h('span', { class: 'dot' }), online ? '已连接' : '未连接'])
      ]),
      delegated ? h('section', { class: 'ui-card control-card', style: { marginBottom: '14px' } }, [
        h('h3', {}, '拨号由 QModem 管理'),
        h('div', { class: 'control-desc' }, 'XGPV3 的网卡与拨号归 QModem（mt5700m.connection.managed=qmodem），本面板只读取状态、执行 AT 操作，避免两个管理器互相覆盖网卡配置。'),
        h('a', { class: 'btn', href: '/cgi-bin/luci/admin/modem/qmodem' }, '打开 QModem 拨号页面')
      ]) : null,
      h('div', { class: 'card-grid', style: { marginBottom: '14px' } }, [
        UI.card('自动拨号', '', h('div', {}, c.enabled === false ? '已禁用' : '已启用')),
        UI.card('网络接口', '', h('div', {}, conn.network || '--')),
        UI.card('APN', '', h('div', {}, c.apn || '自动')),
        UI.card('保存的 IP 协议', '', h('div', {}, protocolNames[c.pdp_type] || '--'))
      ]),
      h('div', { style: { display: delegated ? 'none' : 'flex', gap: '9px', flexWrap: 'wrap', marginBottom: '14px' } }, online ? [
        h('button', { class: 'btn primary', onClick: () => UI.confirmRun(() => Api.call('redial'), '重拨', '5G 连接将短暂中断，模组重新拨号。').then(() => ctx && ctx.refresh()) }, '重拨'),
        h('button', { class: 'btn danger', onClick: () => UI.confirmRun(() => Api.call('disconnect'), '断开连接', '现在断开移动数据连接？', { danger: true }).then(() => ctx && ctx.refresh()) }, '断开连接')
      ] : [
        h('button', { class: 'btn primary', onClick: () => UI.confirmRun(() => Api.call('connect'), '连接', '开始拨号，应用保存的配置。').then(() => ctx && ctx.refresh()) }, '连接')
      ]),
      sessionPanel(loaded.sessionRaw),
      h('div', { class: 'control-section' }, [h('div', { class: 'control-section-head' }, [h('h3', {}, '保存的连接设置'), h('p', {}, 'APN 通常自动检测。保存后重拨以使用新设置。')]), h('section', { class: 'ui-card control-card' }, configForm(config))]),
      h('div', { class: 'control-section' }, [h('div', { class: 'control-section-head' }, [h('h3', {}, '模组数据路径'), h('p', {}, '模组侧拨号与入站路由控制。')]), moduleTools(loaded.connSettings)]),
      pdpPanel(loaded.connRaw),
      UI.details('最近拨号日志', '点击展开加载', h('pre', { class: 'raw' }, '展开后加载…'))
    ]);
    // 日志按需加载
    const d = root.querySelector('.ui-details');
    if (d) d.addEventListener('toggle', () => {
      if (d.open && !d.dataset.loaded) { d.dataset.loaded = '1'; Api.call('log').then(log => { d.querySelector('.raw').textContent = (typeof log === 'string' ? log : '') || '暂无拨号日志。'; }).catch(e => { d.querySelector('.raw').textContent = e.message; }); }
    });
    return root;
  }

  ctx = asyncPage(container, async () => {
    const [conn, config, connAdv, sessionAdv] = await Promise.all([
      Api.call('connection-status'), Api.call('config'),
      Api.call('advanced', { group: 'connection-settings' }),
      Api.call('advanced', { group: 'session' })
    ]);
    loaded.conn = conn; loaded.config = config;
    loaded.connSettings = U.labeledToRaw(connAdv); loaded.sessionRaw = U.labeledToRaw(sessionAdv);
    loaded.connRaw = U.labeledToRaw(await Api.call('advanced', { group: 'connection' }));
    return loaded;
  }, render);
});
