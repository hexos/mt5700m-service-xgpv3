'use strict';
/* 高级设置页：硬件接口 / SIM / 温度 */

window.registerPage('advanced', function (container) {
  const { U, UI } = window;
  let ctx = null;

  function render() {
    const raw = window._advRaw || '';
    const usbRaw = U.section(raw, 'USB mode');
    const interfaceRaw = U.section(raw, 'Interface mode');
    const nic = U.pick(U.section(raw, 'NIC speed'), /\^TDPCIELANCFG:\s*(\d+)/, '');
    const pcieController = U.pick(U.section(raw, 'PCIe controller'), /\^TDPMCFG:\s*(\d+)/, '');
    const hotplug = U.pick(U.section(raw, 'SIM hotplug'), /\^TDSIMHP:\s*(\d+)/, '');
    const simSlot = U.pick(U.section(raw, 'SIM slot'), /\^SCICHG:\s*(\d+)/, '');
    const thermalMatch = U.section(raw, 'Thermal control').match(/\^THERMAUTOFUN:\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/);
    const interfaceMode = U.pick(interfaceRaw, /Mode:\s*(\d+)/, '');
    const usbModeValue = U.pick(usbRaw, /\^SETMODE:\s*(\d+)/, U.pick(usbRaw, /(?:^|\n)(\d+)(?:\n|$)/, '4'));

    const usbMode = UI.select([['0', 'Linux ECM'], ['1', 'Windows NCM'], ['2', 'Linux ECM · Debug'], ['3', 'Windows NCM · Debug'], ['4', 'Linux NCM'], ['5', 'Linux NCM · Debug'], ['6', 'Windows RNDIS'], ['8', 'PPP']], usbModeValue);
    const nicProfile = UI.select([['1', 'RTL8111 · 1 Gbps'], ['2', 'RTL8125 · 2.5 Gbps']], nic);
    const pcieEnabled = UI.select([['1', '启用'], ['0', '禁用']], pcieController || '1');
    const ifaceMode = UI.select([['1', 'USB 卡 + 以太网路由模式'], ['2', 'USB 路由 + 以太网路由模式']], interfaceMode);
    const simHotplug = UI.select([['1', '启用'], ['0', '禁用']], hotplug);
    const thermalEnabled = UI.select([['1', '启用'], ['0', '禁用']], thermalMatch ? thermalMatch[1] : '1');
    const thermalInterval = UI.select([['1', '1 s'], ['2', '2 s'], ['3', '3 s'], ['5', '5 s'], ['10', '10 s'], ['30', '30 s'], ['60', '60 s']], thermalMatch ? thermalMatch[3] : '2');

    return h('div', {}, [
      h('div', { class: 'hero' }, [h('div', {}, [h('h2', {}, '高级设置'), h('div', { class: 'hero-sub' }, '面向经验用户的硬件接口、安全与诊断工具。正常使用无需在此页更改。')])]),
      h('div', { class: 'alert warning' }, '更改接口配置可能中断移动数据与模组管理。应用前请记录当前值。'),
      h('div', { class: 'control-section' }, [
        h('div', { class: 'control-section-head' }, [h('h3', {}, '数据接口'), h('p', {}, '主机 USB 网络驱动与可选 PCIe 以太网路径所使用的配置。')]),
        h('div', { class: 'control-grid' }, [
          UI.card('USB 数据模式', '选择模组呈现的 USB 网络驱动配置。', [
            UI.row('USB 模式', usbMode),
            UI.actionBtn('应用 USB 模式', () => UI.confirmRun(() => Api.call('usb-mode', { value: usbMode.value }), '更改 USB 模式', 'USB 连接可能在模组重启前消失。', { danger: true }))
          ]),
          UI.card('PCIe 以太网', '将 PHY 配置匹配到连接到 MT5700M 的以太网控制器。', [
            UI.row('PCIe 控制器', pcieEnabled),
            UI.actionBtn('应用 PCIe 控制器', () => UI.confirmRun(() => Api.call('pcie-controller', { value: pcieEnabled.value }), 'PCIe 控制器', '禁用 PCIe 控制器会移除模组以太网数据路径。', { danger: true })),
            UI.row('PHY 配置', nicProfile),
            UI.actionBtn('应用 PHY 配置', () => UI.confirmRun(() => Api.call('nic-speed', { value: nicProfile.value }), '更改 PCIe 以太网 PHY', '错误的 PHY 配置可能导致模组以太网链路不可用。', { danger: true }))
          ]),
          UI.card('接口运行模式', '选择 USB 与以太网数据路径之间的关系。', [
            UI.row('接口模式', ifaceMode),
            UI.actionBtn('应用接口模式', () => UI.confirmRun(() => Api.call('interface-mode', { value: ifaceMode.value }), '更改接口模式', '这会改变 USB 与以太网寻址，可能断开当前会话。', { danger: true }))
          ], true)
        ])
      ]),
      h('div', { class: 'control-section' }, [
        h('div', { class: 'control-section-head' }, [h('h3', {}, '模组硬件行为'), h('p', {}, 'SIM 检测与内置温度保护轮询策略。')]),
        h('div', { class: 'control-grid' }, [
          UI.card('SIM 硬件', '查看当前 SIM 路径并配置物理卡变更检测。', [
            UI.stateRow('当前 SIM 卡槽', simSlot === '0' ? '外置 SIM' : simSlot === '1' ? '内置 SIM' : '--'),
            UI.row('SIM 热插拔', simHotplug),
            UI.actionBtn('应用 SIM 热插拔', () => UI.confirmRun(() => Api.call('sim-hotplug', { value: simHotplug.value }), 'SIM 热插拔', '应用所选物理 SIM 检测行为？'))
          ]),
          UI.card('温度保护', '配置 MT5700M 内置温度保护轮询。', [
            UI.row('温度保护', thermalEnabled), UI.row('温度间隔', thermalInterval),
            h('div', { class: 'control-note' }, '正常操作请保持温度保护启用。'),
            UI.actionBtn('应用温度设置', () => UI.confirmRun(() => Api.call('thermal', { enabled: thermalEnabled.value, interval: thermalInterval.value }), '温度保护', '应用这些温度保护设置？'))
          ])
        ])
      ]),
      h('section', { class: 'ui-card control-card', style: { marginTop: '18px', display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: '14px' } }, [
        h('div', {}, [h('h3', {}, '诊断与开发者工具'), h('div', { class: 'control-desc' }, '打开底层通信设置或直接向模组发送 AT 命令。')]),
        h('div', { style: { display: 'flex', gap: '8px', flexWrap: 'wrap' } }, [
          h('a', { class: 'btn', href: '#/settings' }, '通信诊断'),
          h('a', { class: 'btn primary', href: '#/terminal' }, 'AT 控制台')
        ])
      ]),
      UI.details('技术详情', '', h('pre', { class: 'raw' }, raw || '无响应'))
    ]);
  }

  ctx = asyncPage(container, async () => {
    const adv = await Api.call('advanced', { group: 'hardware' });
    window._advRaw = U.labeledToRaw(adv);
    return {};
  }, render);
});
