'use strict';
/* 模组与 SIM 页：身份/运行状态/温度/SIM 控制/FOTA/维护 */

window.registerPage('system', function (container) {
  const { U, UI } = window;
  let ctx = null;

  function lineValue(text, prefix) {
    const line = String(text || '').split(/\n/).filter(l => l.indexOf(prefix) === 0)[0] || '';
    return line.substring(prefix.length).replace(/^[ :]+/, '').trim();
  }
  function subscriptionRate(v) {
    v = Number(v) || 0;
    return v > 0 ? (v / 1000).toFixed(v % 1000 ? 1 : 0) + ' Mbps' : '--';
  }

  function run(title, message, action, danger, recoveryDelay) {
    return UI.confirmRun(async () => { const r = await action(); return r; }, title, message, { danger }).then(ok => { if (ok) setTimeout(() => ctx && ctx.refresh(), recoveryDelay || 1500); });
  }

  function pinManager(simState) {
    const op = UI.select([['verify', '验证当前 PIN'], ['enable', '启用 PIN 锁'], ['disable', '禁用 PIN 锁'], ['change', '更改 PIN'], ['unblock', 'PUK 解锁']], simState === 'SIM PUK' ? 'unblock' : 'verify');
    const first = h('input', { class: 'input', type: 'password', autocomplete: 'off' });
    const second = h('input', { class: 'input', type: 'password', autocomplete: 'new-password' });
    const firstLabel = h('label', {}, 'PIN');
    const secondRow = UI.row('新 PIN', second);
    function update() {
      firstLabel.textContent = op.value === 'unblock' ? 'PUK 码' : op.value === 'change' ? '当前 PIN' : 'PIN';
      secondRow.style.display = (op.value === 'change' || op.value === 'unblock') ? '' : 'none';
    }
    op.addEventListener('change', update);
    setTimeout(update, 0);
    const m = UI.modal('SIM PIN 管理', h('div', {}, [
      h('div', { class: 'alert warning' }, '错误的 PIN/PUK 次数可能导致 SIM 永久锁定。操作前请查阅运营商文档。'),
      UI.row('操作', op), h('div', { class: 'control-row' }, [firstLabel, first]), secondRow
    ]), [
      h('button', { class: 'btn default', onClick: m.close }, '取消'),
      h('button', { class: 'btn danger', onClick: () => {
        const a = first.value.trim(), b = second.value.trim();
        if ((op.value === 'unblock' ? !/^\d{8}$/.test(a) : !/^\d{4,8}$/.test(a)) || ((op.value === 'change' || op.value === 'unblock') && !/^\d{4,8}$/.test(b))) { UI.toast('PIN 需 4–8 位数字；PUK 需 8 位数字', 'warning'); return; }
        m.close();
        UI.confirmRun(() => Api.call('sim-pin', { operation: op.value, a, b }), '应用 PIN 操作', '模组将执行 PIN 操作。', { danger: true }).then(ok => { if (ok) ctx && ctx.refresh(); });
      } }, '应用')
    ]);
  }

  function thermalManager(thresholds, logValues) {
    const labels = ['正常阈值', '一级降功率', '一级恢复', '二级降功率', '二级恢复', '持续限功率', '持续限恢复', '紧急飞行模式', '紧急恢复'];
    const inputs = labels.map((label, i) => h('input', { class: 'input', type: 'number', min: '0', max: '150', value: thresholds[i] || '' }));
    const serial = UI.select([['0', '禁用'], ['1', '启用']], logValues[0] || '0');
    const file = UI.select([['0', '禁用'], ['1', '启用']], logValues[1] || '0');
    const m = UI.modal('温度保护设置', h('div', {}, [
      h('div', { class: 'alert warning' }, '错误阈值会导致性能下降、过热或紧急关机。恢复值须低于对应触发值。'),
      h('div', { class: 'card-grid' }, labels.map((label, i) => UI.row(label + ' (°C)', inputs[i]))),
      UI.row('串口温度日志', serial), UI.row('存储温度日志', file)
    ]), [
      h('button', { class: 'btn default', onClick: m.close }, '取消'),
      h('button', { class: 'btn danger', onClick: () => {
        const next = inputs.map(i => String(i.value || ''));
        if (next.some(v => !/^\d+$/.test(v) || Number(v) > 150)) { UI.toast('阈值须为 0–150°C', 'warning'); return; }
        m.close();
        const vals = next.map((v, i) => 'v' + (i + 1)).reduce((o, k, i) => { o[k] = next[i]; return o; }, {});
        UI.confirmRun(async () => {
          await Api.call('thermal-thresholds', vals);
          await Api.call('thermal-log', { serial: serial.value, file: file.value });
        }, '保存温度设置', '应用温度保护设置？', { danger: true }).then(ok => { if (ok) ctx && ctx.refresh(); });
      } }, '应用')
    ]);
  }

  function identityLab(currentImei) {
    const input = h('input', { class: 'input', maxlength: '15', placeholder: currentImei || '123456789012345' });
    const m = UI.modal('设备身份实验室', h('div', {}, [
      h('div', { class: 'alert warning' }, 'IMEI 写入可能受当地法律或运营商限制。仅用于恢复设备原始身份。'),
      UI.row('新 IMEI', input)
    ]), [
      h('button', { class: 'btn default', onClick: m.close }, '取消'),
      h('button', { class: 'btn danger', onClick: () => {
        const v = input.value.trim();
        if (!/^\d{15}$/.test(v) || v === currentImei) { UI.toast('请输入不同的 15 位 IMEI', 'warning'); return; }
        m.close();
        UI.confirmRun(() => Api.call('set-imei', { imei: v }), '写入设备身份', '此不支持的操作可能导致无法注册网络。', { danger: true });
      } }, '审核更改')
    ]);
  }

  function factoryReset() {
    const input = h('input', { class: 'input', placeholder: 'RESET' });
    const m = UI.modal('恢复模组出厂设置', h('div', {}, [
      h('div', { class: 'alert warning' }, '此操作会清除模组侧 APN、无线、SIM、接口与温度设置，然后重启 MT5700M。OpenWrt 设置不受影响。'),
      UI.row('输入 RESET 确认', input)
    ]), [
      h('button', { class: 'btn default', onClick: m.close }, '取消'),
      h('button', { class: 'btn danger', onClick: () => {
        if (input.value !== 'RESET') { UI.toast('确认文本不匹配', 'warning'); return; }
        m.close();
        UI.confirmRun(() => Api.call('factory-reset'), '恢复出厂设置', '确认恢复模组出厂设置？', { danger: true });
      } }, '恢复出厂设置')
    ]);
  }

  function render() {
    const raw = window._sysRaw || '';
    const identity = U.section(raw, 'Identity');
    const version = U.section(raw, 'Version');
    const sim = lineValue(U.section(raw, 'SIM'), '+CPIN');
    const iccid = lineValue(U.section(raw, 'ICCID'), '^ICCID');
    const imsi = (U.section(raw, 'IMSI').match(/(?:^|\n)(\d{10,18})(?:\n|$)/) || [, ''])[1];
    const phone = lineValue(U.section(raw, 'Subscriber number'), '+CNUM').replace(/"/g, '').split(',').map(v => v.trim()).filter(v => /^\+?\d{5,20}$/.test(v))[0] || '未存储在 SIM';
    const sub = lineValue(U.section(raw, 'Subscription rate'), '^DSAMBR').replace(/"/g, '').split(',').map(v => v.trim());
    const opV = lineValue(U.section(raw, 'Operator'), '+COPS').replace(/"/g, '').split(',');
    const networkTime = lineValue(U.section(raw, 'Network time'), '^NWTIME').replace(/"/g, '');
    const functionLevel = lineValue(U.section(raw, 'Function level'), '+CFUN');
    const ledState = lineValue(U.section(raw, 'LED'), '^LEDSWITCH');
    const simAct = lineValue(U.section(raw, 'SIM activation'), '^HVSST').split(',');
    const simSlots = lineValue(U.section(raw, 'SIM slot'), '^SCICHG').split(',');
    const temps = lineValue(U.section(raw, 'Temperature'), '^CHIPTEMP').split(',').map(v => v.trim()).filter(v => /^-?\d+$/.test(v) && Number(v) > -1000 && Number(v) < 2000).map(Number);
    const temperature = temps.length ? (Math.max.apply(null, temps) / 10).toFixed(1) : '';
    const fotaMode = lineValue(U.section(raw, 'FOTA mode'), '^FOTAMODE');
    const fotaModeName = fotaMode === '0,1,0,1' ? 'HTTP 更新模式' : fotaMode;
    const fotaState = lineValue(U.section(raw, 'FOTA state'), '^FOTASTATE');
    const fotaProg = lineValue(U.section(raw, 'FOTA progress'), '^FOTADLQ').replace(/"/g, '').split(',');
    const total = Number(fotaProg[fotaProg.length - 2]) || 0;
    const received = Number(fotaProg[fotaProg.length - 1]) || 0;
    const percent = total > 0 ? Math.max(0, Math.min(100, Math.floor(received / total * 100))) : 0;
    const revision = lineValue(identity, 'Revision') || lineValue(U.section(raw, 'Revision'), '');
    const imei = lineValue(identity, 'IMEI') || U.section(raw, 'IMEI').split(/\n/)[0];
    const buildDate = lineValue(version, '^VERSION:BDT');
    const software = lineValue(version, '^VERSION:EXTS');
    const hardware = lineValue(version, '^VERSION:EXTH');
    const thermalStatus = lineValue(U.section(raw, 'Thermal status'), '^THERMLDAUTOSTATUS') || lineValue(U.section(raw, 'Thermal status'), '+THERMLDAUTOSTATUS');
    const thermalValues = thermalStatus.split(',').map(v => v.trim());
    const thermalLevel = Number(thermalValues[5]) || 0;
    const thermalThresholds = lineValue(U.section(raw, 'Thermal thresholds'), '^THERMLDAUTOPARA').split(',').map(v => v.trim());
    const thermalLog = lineValue(U.section(raw, 'Thermal log'), '^THERMLDLOGSW').replace(/\s+/g, ',').split(',');
    const stateNames = { '10': '等待下载', '11': '检查更新', '12': '有可用更新', '13': '检查失败', '14': '无可用更新', '20': '下载失败', '30': '下载中', '31': '下载暂停', '40': '下载完成', '50': '准备安装' };
    const stateName = fotaState ? (stateNames[fotaState] || '未知状态') : '状态不可用';
    const stateClass = fotaState === '30' || fotaState === '40' || fotaState === '50' ? ' active' : fotaState === '13' || fotaState === '20' ? ' error' : '';
    const fotaUrl = h('input', { class: 'input', placeholder: 'http://server/path/' });
    const ledSel = UI.select([['1', '启用'], ['0', '禁用']], ledState || '1');
    const simEnabled = UI.select([['1', '激活'], ['0', '未激活']], simAct[1] || '1');
    const simSlot = UI.select([['0', 'SIM 卡槽 0'], ['1', 'SIM 卡槽 1']], simSlots[0] || simAct[2] || '0');

    const root = h('div', {}, [
      h('div', { class: 'hero' }, [
        h('div', {}, [h('div', { class: 'hero-kicker' }, 'MT5700M SYSTEM'), h('h2', {}, 'MT5700M'), h('div', { class: 'hero-sub' }, revision || '模组信息')]),
        h('div', { class: 'status-pill' }, [h('strong', { style: { fontSize: '22px' } }, temperature ? temperature + '°' : '--'), h('span', {}, '峰值温度')])
      ]),
      h('div', { class: 'card-grid' }, [
        h('section', { class: 'ui-card control-card' }, [h('h3', {}, '模组信息'), UI.kv('型号', 'MT5700M'), UI.kv('固件版本', software || revision), UI.kv('硬件版本', hardware), UI.kv('IMEI', imei), UI.kv('构建日期', buildDate)]),
        h('section', { class: 'ui-card control-card' }, [h('h3', {}, 'SIM 与订阅'), UI.kv('电话号码', phone), UI.kv('ICCID', iccid), UI.kv('IMSI', imsi), UI.kv('订阅下行', subscriptionRate(sub[1])), UI.kv('订阅上行', subscriptionRate(sub[2]))]),
        h('section', { class: 'ui-card control-card' }, [h('h3', {}, '运行状态'), UI.kv('SIM 状态', sim), UI.kv('运营商', opV[2] || '--'), UI.kv('网络时间', networkTime), UI.kv('无线功能', functionLevel === '0' ? '飞行模式' : functionLevel === '1' ? '在线' : functionLevel), UI.kv('SIM 供电', simAct[1] === '1' ? '激活' : simAct.length > 1 ? '未激活' : ''), UI.kv('FOTA 模式', fotaModeName)]),
        h('section', { class: 'ui-card control-card' }, [h('h3', {}, '温度保护状态'), UI.kv('保护等级', thermalValues.length ? (thermalLevel === 0 ? '正常' : '等级 ' + thermalLevel) : ''), UI.kv('一级降功率', thermalThresholds[1] ? thermalThresholds[1] + '°C / ' + thermalThresholds[2] + '°C' : ''), UI.kv('二级降功率', thermalThresholds[3] ? thermalThresholds[3] + '°C / ' + thermalThresholds[4] + '°C' : ''), UI.kv('持续限功率', thermalThresholds[5] ? thermalThresholds[5] + '°C / ' + thermalThresholds[6] + '°C' : ''), UI.kv('紧急飞行模式', thermalThresholds[7] ? thermalThresholds[7] + '°C / ' + thermalThresholds[8] + '°C' : ''), UI.kv('温度日志', thermalLog[0] === '1' || thermalLog[1] === '1' ? '已启用' : thermalLog.length > 1 ? '已禁用' : '')]),
        h('section', { class: 'ui-card control-card wide' }, [
          h('h3', {}, 'SIM 与无线'),
          UI.stateRow('当前无线状态', functionLevel === '0' ? '飞行模式' : '在线'),
          UI.actionBtn(functionLevel === '0' ? '恢复移动无线' : '进入飞行模式', () => run('更改无线功能', functionLevel === '0' ? '恢复移动注册与数据服务？' : '飞行模式会立即断开移动数据与语音。', () => Api.call('airplane', { state: functionLevel === '0' ? '1' : '0' }), functionLevel !== '0')),
          UI.row('模组状态 LED', ledSel),
          UI.actionBtn('应用 LED 设置', () => UI.confirmRun(() => Api.call('led', { value: ledSel.value }), '模组状态 LED', 'LED 设置由模组存储，重启后生效。', { danger: true })),
          UI.actionBtn('管理 SIM PIN', () => pinManager(sim)),
          UI.row('SIM 激活', simEnabled),
          UI.actionBtn('应用 SIM 激活', () => UI.confirmRun(() => Api.call('sim-activation', { value: simEnabled.value }), 'SIM 激活', simEnabled.value === '1' ? '激活物理 SIM 以注册网络？' : '停用 SIM 会立即移除移动服务。', { danger: simEnabled.value === '0' })),
          UI.row('当前 SIM 卡槽', simSlot),
          UI.actionBtn('切换 SIM 卡槽', () => UI.confirmRun(() => Api.call('sim-slot', { value: simSlot.value }), '切换 SIM 卡槽', '切换物理 SIM 路径时 MT5700M 会脱离网络。', { danger: true }))
        ])
      ]),
      h('section', { class: 'ui-card control-card', style: { marginTop: '14px', display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: '14px' } }, [
        h('div', {}, [h('h3', {}, '保护与维护'), h('div', { class: 'control-desc' }, '模组保护、恢复与通信排障工具。')]),
        h('div', { style: { display: 'flex', gap: '8px', flexWrap: 'wrap' } }, [
          h('a', { class: 'btn', href: '#/settings' }, '通信诊断'),
          h('button', { class: 'btn', onClick: () => thermalManager(thermalThresholds, thermalLog) }, '配置温度保护'),
          h('button', { class: 'btn', onClick: () => identityLab(imei) }, '设备身份实验室'),
          h('button', { class: 'btn danger', onClick: () => factoryReset() }, '恢复出厂设置')
        ])
      ]),
      h('section', { class: 'fota-box' }, [
        h('div', { class: 'fota-head' }, [h('div', {}, [h('h3', {}, '固件更新'), h('p', {}, '使用兼容的 HTTP 更新服务器。安装期间请勿断电。')]), h('span', { class: 'badge' + stateClass }, stateName)]),
        h('div', { class: 'progress' }, h('span', { style: { width: percent + '%' } })),
        h('div', { class: 'progress-label' }, percent + '% 完成'),
        h('div', { style: { display: 'grid', gridTemplateColumns: '1fr auto', gap: '9px', marginTop: '14px' } }, [
          fotaUrl,
          h('button', { class: 'btn primary', onClick: () => {
            if (!/^http:\/\//.test(fotaUrl.value || '')) { UI.toast('请输入有效的 HTTP 更新服务器 URL', 'warning'); return; }
            run('开始固件下载', '模组将联系指定服务器并可能使用移动数据。', () => Api.call('fota-start', { url: fotaUrl.value }), false);
          } }, '检查并下载')
        ]),
        h('div', { style: { display: 'flex', gap: '9px', flexWrap: 'wrap', marginTop: '14px' } }, [
          h('button', { class: 'btn', onClick: () => ctx && ctx.refresh() }, '刷新状态'),
          h('button', { class: 'btn', disabled: fotaState !== '31', onClick: () => run('恢复下载', '恢复暂停的固件下载？', () => Api.call('fota-resume'), false) }, '恢复下载'),
          h('button', { class: 'btn danger', disabled: fotaState !== '40', onClick: () => run('安装固件', '模组将重启，安装完成前请勿断电。', () => Api.call('fota-upgrade'), true, 24000) }, '安装更新'),
          h('button', { class: 'btn danger', onClick: () => run('重启模组', '这将重启 MT5700M 并短暂中断 5G 连接。', () => Api.call('restart'), true, 24000) }, '重启模组')
        ])
      ]),
      UI.details('技术详情', '', h('pre', { class: 'raw' }, raw || '无响应'))
    ]);
    return root;
  }

  ctx = asyncPage(container, async () => {
    const sys = await Api.call('system');
    window._sysRaw = U.labeledToRaw(sys);
    return {};
  }, render);
});
