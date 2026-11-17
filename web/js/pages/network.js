'use strict';
/* 无线与小区页：服务小区 + 无线状态 + 诊断 + 锁频 + 无线偏好 */

window.registerPage('network', function (container) {
  const { U, UI } = window;
  let ctx = null;

  function bandChecklist(options, mask, anyMask) {
    const numeric = parseInt(mask || '0', 16);
    const all = String(mask || '').toUpperCase() === anyMask;
    const checks = options.map(item => h('input', { type: 'checkbox', value: item[0], checked: all || (numeric && Math.floor(numeric / parseInt(item[0], 16)) % 2 === 1) }));
    const node = h('div', { class: 'band-options' }, options.map((item, i) => h('label', { class: 'band-option' }, [checks[i], h('span', {}, item[1])])));
    node._checks = checks;
    return node;
  }
  function selectedBandMask(node, anyMask) {
    const sel = node._checks.filter(c => c.checked);
    if (!sel.length) return '';
    if (sel.length === node._checks.length) return anyMask;
    return sel.reduce((t, c) => t + parseInt(c.value, 16), 0).toString(16).toUpperCase();
  }
  function bandPanel(title, desc, checklist) {
    return h('section', { class: 'ui-card band-card' }, [
      h('div', { class: 'band-head' }, [h('div', {}, [h('h3', {}, title), h('p', {}, desc)]), h('button', { class: 'btn', onClick: () => checklist._checks.forEach(c => c.checked = true) }, '全选')]),
      checklist
    ]);
  }

  function lockPanel(title, rat, currentType) {
    const type = UI.select([['3', '频段锁定'], ['1', 'ARFCN 锁定'], ['2', '小区锁定'], ['0', '移除锁定']], /^(0|1|2|3)$/.test(currentType || '') ? currentType : '0');
    const bands = h('input', { class: 'input', placeholder: rat === 'nr' ? '78,41' : '3,8' });
    const arfcns = h('input', { class: 'input', placeholder: rat === 'nr' ? '630000,520000' : '1850,3450' });
    const scs = h('input', { class: 'input', placeholder: '1,1' });
    const pcis = h('input', { class: 'input', placeholder: '100,200' });
    const wraps = {};
    function field(key, label, input, help) {
      wraps[key] = h('div', { class: 'freq-field' }, [h('label', {}, label), input, h('div', { class: 'freq-help' }, help)]);
      return wraps[key];
    }
    function update() {
      const t = type.value;
      wraps.bands.style.display = t === '0' ? 'none' : '';
      wraps.arfcns.style.display = (t === '1' || t === '2') ? '' : 'none';
      if (wraps.scs) wraps.scs.style.display = (t === '1' || t === '2') ? '' : 'none';
      wraps.pcis.style.display = t === '2' ? '' : 'none';
    }
    function apply() {
      const t = type.value;
      const values = [U.cleanCsv(bands.value), U.cleanCsv(arfcns.value), U.cleanCsv(scs.value), U.cleanCsv(pcis.value)];
      const required = t === '0' ? [] : t === '3' ? [0] : t === '1' ? (rat === 'nr' ? [0, 1, 2] : [0, 1]) : (rat === 'nr' ? [0, 1, 2, 3] : [0, 1, 3]);
      if (required.some(i => !U.validCsv(values[i]))) { UI.toast('请用逗号分隔的数字填全必填项', 'warning'); return; }
      const lengths = required.map(i => values[i].split(',').length);
      if (lengths.some(n => n !== lengths[0])) { UI.toast('每个字段的取值个数必须一致', 'warning'); return; }
      if (lengths[0] > 20) { UI.toast('最多 20 条锁定记录', 'warning'); return; }
      const args = { rat, type: t, bands: values[0], arfcns: values[1], scs: values[2], pcis: values[3] };
      UI.confirmRun(() => Api.call('lock', args), t === '0' ? '移除锁定' : '应用锁定', (t === '0' ? '移除当前 ' + rat.toUpperCase() + ' 频段锁定？' : '应用此 ' + rat.toUpperCase() + ' 频段锁定？移动连接可能重连。') + ' 切换期间飞行模式会短暂断开服务。', { danger: true }).then(ok => { if (ok) ctx && ctx.refresh(); });
    }
    type.addEventListener('change', update);
    const body = [
      field('type', '锁定类型', type, '选择满足需求的最小限制模式。'),
      field('bands', '频段', bands, '逗号分隔的数字。'),
      field('arfcns', 'ARFCN', arfcns, '每个频段一个 ARFCN。')
    ];
    if (rat === 'nr') body.push(field('scs', 'SCS 类型', scs, '每个 NR 频段一个 SCS 类型。'));
    body.push(field('pcis', 'PCI', pcis, '每个频段与 ARFCN 一个 PCI。'), h('div', { class: 'freq-actions' }, h('button', { class: 'btn primary', onClick: apply }, '审核并应用')));
    const card = h('section', { class: 'ui-card freq-card' }, [h('h4', {}, title)].concat(body));
    setTimeout(update, 0);
    return card;
  }

  function radioDiagnostics(raw) {
    const fmtMcs = vals => {
      if (!vals.length) return '';
      const rat = vals[2] === '1' ? 'NR' : vals[2] === '0' ? 'LTE' : '';
      const cw = vals.slice(3).filter(v => /^\d+$/.test(v) && v !== '255');
      return [rat, cw.length ? 'MCS ' + cw.join(' / ') : ''].filter(Boolean).join(' · ');
    };
    const uplinkMcs = U.matchValues(U.section(raw, 'Uplink MCS'), '^MCS');
    const downlinkMcs = U.matchValues(U.section(raw, 'Downlink MCS'), '^MCS');
    const txPower = U.matchValues(U.section(raw, 'NR transmit power'), '^NTXPOWER');
    const ssb = U.matchValues(U.section(raw, 'NR SSB beam'), '^NRSSBID');
    const qos = U.matchValues(U.section(raw, 'QoS'), '+CGEQOSRDP');
    const reg = U.matchValues(U.section(raw, 'Data registration'), '+C5GREG');
    const ims = U.matchValues(U.section(raw, 'IMS registration'), '+CIREG');
    const endc = U.matchValues(U.section(raw, 'Dual connectivity'), '^LENDC');
    return h('div', { class: 'card-grid', style: { marginTop: '12px' } }, [
      h('section', { class: 'ui-card control-card' }, [h('h3', {}, '无线链路详情'), UI.kv('上行调制', fmtMcs(uplinkMcs)), UI.kv('下行调制', fmtMcs(downlinkMcs)), UI.kv('QoS 等级', qos[1] ? 'QCI ' + qos[1] : ''), UI.kv('NR PUSCH 功率', txPower[0] && txPower[0] !== '999' ? txPower[0] + ' dBm' : ''), UI.kv('NR 发射频率', txPower[4] && txPower[4] !== '0' ? (Number(txPower[4]) / 1000).toFixed(1) + ' MHz' : '')]),
      h('section', { class: 'ui-card control-card' }, [h('h3', {}, '5G 波束与服务'), UI.kv('SSB ARFCN', ssb[0]), UI.kv('服务波束 PCI', ssb[2]), UI.kv('波束 RSRP', ssb[3] ? ssb[3] + ' dBm' : ''), UI.kv('波束 SINR', ssb[4] ? ssb[4] + ' dB' : ''), UI.kv('数据注册', reg[1] === '1' || reg[1] === '5' ? '已注册' : reg.length ? '未注册' : ''), UI.kv('IMS 注册', ims[1] === '1' ? '已注册' : ims.length ? '未注册' : ''), UI.kv('LTE-NR 双连接', endc[0] === '1' ? '已启用' : endc.length ? '已禁用' : '')])
    ]);
  }

  function radioPreferences(raw) {
    const sys = U.matchValues(U.section(raw, 'Radio mode'), '^SYSCFGEX');
    const radioCode = sys[0] || '';
    const wcdmaMask = sys[1] || '3FFFFFFF';
    const roam = sys[2] || '1';
    const service = sys[3] || '2';
    const lteMask = sys[4] || '7FFFFFFFFFFFFFFF';
    const radioLabels = { '00': '自动', '01': 'GSM', '02': 'WCDMA', '03': 'LTE', '08': '5G NR', '0302': 'LTE / WCDMA', '030201': 'LTE / WCDMA / GSM', '0803': '5G NR / LTE', '080302': '5G NR / LTE / WCDMA' };
    const radioMode = (radioLabels[radioCode] ? radioLabels[radioCode] + ' · ' + radioCode : radioCode) || '未读';
    const radioModeSelect = UI.select([['080302', '5G NR / LTE / WCDMA（推荐）'], ['0803', '5G NR / LTE'], ['08', '仅 5G NR'], ['03', '仅 LTE'], ['0302', 'LTE / WCDMA'], ['02', 'WCDMA']], radioCode || '080302');
    const roaming = UI.select([['0', '仅归属网络'], ['1', '允许漫游']], roam);
    const srv = UI.select([['1', '仅数据服务'], ['2', '语音与数据服务']], service);
    const wcdmaBands = bandChecklist([['400000', 'B1 · 2100 MHz'], ['2000000000000', 'B8 · 900 MHz']], wcdmaMask, '3FFFFFFF');
    const lteBands = bandChecklist([['1', 'B1'], ['4', 'B3'], ['10', 'B5'], ['80', 'B8'], ['200000000', 'B34'], ['2000000000', 'B38'], ['4000000000', 'B39'], ['8000000000', 'B40'], ['10000000000', 'B41']], lteMask, '7FFFFFFFFFFFFFFF');
    const access = U.matchValues(U.section(raw, '5G access mode'), '^C5GOPTION');
    const accessCode = access.slice(0, 3).join(',');
    const accessPreset = UI.select([['option23', 'SA + NSA（Option 2 + 3）'], ['option2', '仅 SA（Option 2）'], ['option3', '仅 NSA（Option 3）']], accessCode === '1,0,1' ? 'option2' : accessCode === '0,1,0' ? 'option3' : 'option23');
    const ca = U.pick(U.section(raw, 'NR carrier aggregation'), /\^NRRCCAPQRY:\s*3,(\d+)/, '');
    const vonr = U.pick(U.section(raw, 'VoNR'), /\^NRRCCAPQRY:\s*2,(\d+)/, '');
    const dssM = U.section(raw, 'DSS').match(/\^NRRCCAPQRY:\s*5,(\d+),(\d+)/);
    const caSel = UI.select([['1', '启用'], ['0', '禁用']], ca);
    const vonrSel = UI.select([['0', '禁用'], ['1', 'FR1 VoNR'], ['2', 'FR2 VoNR'], ['3', 'FR1 + FR2 VoNR']], vonr);
    const dssRate = UI.select([['0', '保持出厂能力'], ['1', '强制关闭']], dssM ? dssM[1] : '0');
    const dssDmrs = UI.select([['0', '保持出厂能力'], ['1', '强制关闭']], dssM ? dssM[2] : '0');

    return h('div', { class: 'control-grid' }, [
      UI.card('网络接入策略', '选择无线优先级、漫游与服务域。', [
        UI.row('无线接入顺序', radioModeSelect), UI.row('漫游策略', roaming), UI.row('服务域', srv)
      ]),
      bandPanel('WCDMA 频段', '选择模组可用的 WCDMA 频段。', wcdmaBands),
      bandPanel('LTE 频段', '选择模组可用的 LTE 频段。', lteBands),
      h('section', { class: 'ui-card band-apply' }, [
        h('p', {}, '正常使用保持全选。限制频段可能导致旅行时无法注册。'),
        h('button', { class: 'btn primary', onClick: () => {
          const w = selectedBandMask(wcdmaBands, '3FFFFFFF'), l = selectedBandMask(lteBands, '7FFFFFFFFFFFFFFF');
          if (!w || !l) { UI.toast('请至少选择一个 WCDMA 和一个 LTE 频段', 'warning'); return; }
          UI.confirmRun(() => Api.call('radio-policy', { acqorder: radioModeSelect.value, band: w, roam: roaming.value, srvdomain: srv.value, lteband: l }), '更改网络策略', '所选无线制式或频段不可用时，模组可能失去服务。', { danger: true }).then(ok => { if (ok) ctx && ctx.refresh(); });
        } }, '应用网络与频段设置')
      ]),
      UI.card('5G 接入架构', '选择模组使用独立组网、非独立组网或两者。', [
        UI.row('5G 接入模式', accessPreset),
        h('div', { class: 'control-note' }, '更改后需飞行模式循环 + 模组重启。'),
        UI.actionBtn('应用 5G 接入模式', () => UI.confirmRun(() => Api.call('5g-access', { preset: accessPreset.value }), '更改 5G 接入模式', '飞行模式期间移动服务会短暂断开。', { danger: true }))
      ]),
      UI.card('5G 服务能力', '模组通告的载波聚合与语音能力。', [
        UI.stateRow('当前无线制式', radioMode),
        UI.row('NR 载波聚合能力', caSel),
        UI.actionBtn('应用载波聚合', () => UI.confirmRun(() => Api.call('carrier-aggregation', { value: caSel.value }), 'NR 载波聚合能力', '应用所选载波聚合能力？', { danger: true })),
        UI.row('VoNR 模式', vonrSel),
        UI.actionBtn('应用 VoNR 模式', () => UI.confirmRun(() => Api.call('vonr', { value: vonrSel.value }), 'VoNR 模式', '应用所选 VoNR 能力？', { danger: true }))
      ]),
      UI.card('DSS 兼容性', '仅在移动网络要求时限制可选 DSS 能力。', [
        UI.row('DSS 速率匹配能力', dssRate), UI.row('附加 DMRS 能力', dssDmrs),
        h('div', { class: 'control-note' }, '强制关闭是兼容性覆盖；正常操作保持出厂能力。'),
        UI.actionBtn('应用 DSS 设置', () => UI.confirmRun(() => Api.call('dss', { rate: dssRate.value, dmrs: dssDmrs.value }), 'DSS', '应用所选 DSS 能力限制？', { danger: true }))
      ])
    ]);
  }

  function render() {
    const net = window._net || {};
    const raw = net.raw || '';
    const signal = U.matchValues(U.section(raw, 'Signal'), '^HCSQ');
    const cell = U.parseServingCell(U.matchValues(U.section(raw, 'Serving cell'), '^MONSC'));
    const reg = U.matchValues(U.section(raw, 'Network registration'), '+CEREG');
    const op = U.matchValues(U.section(raw, 'Operator'), '+COPS');
    const lteLock = U.matchValues(U.section(raw, 'LTE lock'), '^LTEFREQLOCK');
    const nrLock = U.matchValues(U.section(raw, 'NR lock'), '^NRFREQLOCK');
    const rrc = U.matchValues(U.section(raw, 'RRC state'), '^RRCSTAT');
    const rrcLabels = ['空闲', '连接', '非活动', '无效'];
    let rrcState = rrc.length > 1 ? (rrcLabels[Number(rrc[1])] || rrc[1]) : '';
    if (rrc.length > 2) rrcState += rrc[2] === '98' ? ' · 驻留' : rrc[2] === '99' ? ' · 未驻留' : '';
    const registered = reg[1] === '1' || reg[1] === '5';
    const operatorName = op[2] || '移动网络';

    const root = h('div', {}, [
      h('div', { class: 'hero' }, [
        h('div', {}, [h('div', { class: 'hero-kicker' }, 'NETWORK AND CELL'), h('h2', {}, operatorName), h('div', { class: 'hero-sub' }, '模组上报的服务小区与注册信息。')]),
        h('span', { class: 'badge' + (registered ? ' active' : '') }, registered ? '已注册' : '未注册')
      ]),
      h('div', { class: 'metric-grid', dataset: { sharedSignal: '' }, style: { marginBottom: '14px' } }, [
        UI.metric('RSRP', '--', 'dBm'), UI.metric('RSRQ', '--', 'dB'), UI.metric('SINR', '--', 'dB')
      ]),
      h('div', { class: 'card-grid' }, [
        h('section', { class: 'ui-card control-card' }, [h('h3', {}, '服务小区'),
          UI.kv('无线接入', cell.rat || signal[0]), UI.kv('MCC / MNC', cell.mcc && cell.mnc ? cell.mcc + ' / ' + cell.mnc : ''),
          UI.kv('ARFCN', cell.arfcn), UI.kv('PCI', cell.pci), UI.kv('小区 ID', cell.cellId), UI.kv('TAC / LAC', cell.tac),
          cell.scs ? UI.kv('SCS 类型', cell.scs + ' · ' + (['15', '30', '60', '120', '240'][Number(cell.scs)] || '?') + ' kHz') : null,
          UI.kv('注册', registered ? (reg[1] === '5' ? '漫游' : '归属网络') : '未注册')]),
        h('section', { class: 'ui-card control-card' }, [h('h3', {}, '无线状态'),
          UI.kv('运营商', operatorName), UI.kv('RRC 状态', rrcState),
          UI.kv('LTE 锁定', !lteLock[0] ? '--' : lteLock[0] === '0' ? '未锁定' : '已锁定'),
          UI.kv('NR 锁定', !nrLock[0] ? '--' : nrLock[0] === '0' ? '未锁定' : '已锁定')])
      ]),
      UI.details('无线链路诊断（按需读取）', '', h('div', { class: 'lazy-body' })),
      h('div', { style: { display: 'flex', gap: '9px', margin: '14px 0' } }, [
        h('button', { class: 'btn primary', onClick: () => ctx && ctx.refresh() }, '刷新状态'),
        h('button', { class: 'btn', onClick: () => UI.toast('邻区扫描暂未实现，需后端 cellscan 支持', 'warning') }, '邻区扫描')
      ]),
      UI.details('技术详情', '', h('pre', { class: 'raw' }, raw || '无响应')),
      UI.details('无线偏好 · 修改可能中断蜂窝网络', '', h('div', { class: 'lazy-body' })),
      h('section', { class: 'ui-card freq-head' }, [h('h3', {}, '频段与小区选择'), h('p', {}, '限制 LTE 或 5G NR 频段、频率与小区的进阶控制。正常自动选网请保持解锁。')]),
      h('div', { class: 'freq-grid' }, [lockPanel('LTE 网络', 'lte', lteLock[0]), lockPanel('5G NR 网络', 'nr', nrLock[0])])
    ]);

    // 按需加载诊断与无线偏好
    const lazyBoxes = root.querySelectorAll('.lazy-body');
    const diagDetails = root.querySelectorAll('.ui-details')[0];
    const radioDetails = root.querySelectorAll('.ui-details')[2];
    diagDetails.addEventListener('toggle', () => {
      if (diagDetails.open && !diagDetails.dataset.loaded) { diagDetails.dataset.loaded = '1'; Api.call('advanced', { group: 'radio-diagnostics' }).then(r => { lazyBoxes[0].replaceChildren(radioDiagnostics(U.labeledToRaw(r))); }).catch(e => { lazyBoxes[0].replaceChildren(h('div', { class: 'read-error' }, e.message)); }); }
    });
    radioDetails.addEventListener('toggle', () => {
      if (radioDetails.open && !radioDetails.dataset.loaded) { radioDetails.dataset.loaded = '1'; Api.call('advanced', { group: 'radio' }).then(r => { lazyBoxes[1].replaceChildren(radioPreferences(U.labeledToRaw(r))); }).catch(e => { lazyBoxes[1].replaceChildren(h('div', { class: 'read-error' }, e.message)); }); }
    });

    // 信号轮询
    if (!root.dataset.watching) {
      root.dataset.watching = '1';
      function tick() {
        if (!root.isConnected) return;
        Api.call('signal').then(sig => {
          const box = root.querySelector('[data-shared-signal]');
          if (box) box.replaceChildren(UI.metric('RSRP', sig.rsrp != null ? sig.rsrp : '--', 'dBm'), UI.metric('RSRQ', sig.rsrq != null ? sig.rsrq : '--', 'dB'), UI.metric('SINR', sig.sinr != null ? sig.sinr : '--', 'dB'));
        }).catch(() => {});
      }
      setInterval(() => { if (root.isConnected) tick(); }, 5000);
      tick();
    }
    return root;
  }

  ctx = asyncPage(container, async () => {
    const net = await Api.call('network');
    window._net = { raw: U.labeledToRaw(net) };
    return window._net;
  }, render);
});
