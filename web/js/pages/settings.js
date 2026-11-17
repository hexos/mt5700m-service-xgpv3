'use strict';
/* 通信诊断页：AT 通道配置 */

window.registerPage('settings', function (container) {
  const { UI } = window;
  let ctx = null;

  function render(config) {
    const s = config.settings || {};
    const enabled = h('input', { type: 'checkbox', checked: s.enabled !== false });
    const mode = UI.select([['auto', '自动（推荐）'], ['serial', '串口'], ['network', '网络 TCP']], s.mode || 'auto');
    const atPort = h('input', { class: 'input', placeholder: '自动检测 PCUI 接口', value: s.at_port || '' });
    const host = h('input', { class: 'input', value: s.host || '192.168.8.1' });
    const port = h('input', { class: 'input', type: 'number', value: s.port || '20249' });
    const timeout = h('input', { class: 'input', type: 'number', min: '1', max: '60', value: s.timeout || '8' });

    function update() {
      atPort.closest('.control-row').style.display = mode.value === 'serial' ? '' : 'none';
      host.closest('.control-row').style.display = mode.value === 'network' ? '' : 'none';
      port.closest('.control-row').style.display = mode.value === 'network' ? '' : 'none';
    }
    mode.addEventListener('change', update);

    function save() {
      UI.confirmRun(async () => {
        const sets = [
          ['enabled', enabled.checked ? '1' : '0'], ['mode', mode.value], ['at_port', atPort.value.trim()],
          ['host', host.value.trim()], ['port', port.value], ['timeout', timeout.value]
        ];
        for (const [k, v] of sets) await Api.call('config-set', { section: 'settings', key: k, value: v });
      }, '保存通信设置', '保存 AT 通道设置？更改可能影响模组连接。').then(ok => { if (ok) ctx && ctx.refresh(); });
    }

    const root = h('div', {}, [
      h('div', { class: 'hero' }, [
        h('div', {}, [h('h2', {}, '通信诊断'), h('div', { class: 'hero-sub' }, '用于排障的底层 AT 通道设置。')]),
        h('span', { class: 'badge' }, '推荐自动模式')
      ]),
      h('section', { class: 'ui-card control-card' }, [
        h('div', { class: 'control-section-head' }, [h('h3', {}, 'AT 通信'), h('p', {}, '这些设置不改变 APN 或移动数据。自动检测失败前保持默认。')]),
        h('div', { class: 'control-body' }, [
          h('div', { class: 'control-row' }, [h('label', {}, '启用模组管理'), enabled]),
          h('div', { class: 'control-row' }, [h('label', {}, '连接方式'), mode]),
          h('div', { class: 'control-row' }, [h('label', {}, 'AT 串口'), atPort]),
          h('div', { class: 'control-row' }, [h('label', {}, 'AT 主机'), host]),
          h('div', { class: 'control-row' }, [h('label', {}, 'AT 端口'), port]),
          h('div', { class: 'control-row' }, [h('label', {}, '响应超时（秒）'), timeout]),
          UI.actionBtn('保存通信设置', save)
        ])
      ]),
      h('div', { style: { marginTop: '14px' } }, h('a', { class: 'btn', href: '#/system' }, '返回模组与 SIM'))
    ]);
    setTimeout(update, 0);
    return root;
  }

  ctx = asyncPage(container, () => Api.call('config'), render);
});
