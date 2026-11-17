'use strict';
/* AT 控制台页 */

window.registerPage('terminal', function (container) {
  const { UI } = window;
  const SAVED_KEY = 'mt5700m.at.saved';

  function loadSaved() { try { return JSON.parse(localStorage.getItem(SAVED_KEY) || '[]'); } catch (e) { return []; } }

  const input = h('input', { class: 'input', type: 'text', placeholder: '输入 AT 命令，例如 AT^HCSQ?' });
  const output = h('pre', { class: 'output' }, '就绪。');
  const saved = h('div', { class: 'quick' });

  function appendOutput(node, text, prefix) {
    const ts = new Date().toLocaleTimeString();
    node.textContent += `[${ts}] ${prefix} ${text || ''}\n`;
    node.scrollTop = node.scrollHeight;
  }

  function sendCommand() {
    const cmd = (input.value || '').trim();
    if (!cmd) { UI.toast('命令为空', 'warning'); return; }
    appendOutput(output, cmd, '>>>');
    input.value = '';
    Api.at(cmd).then(res => {
      appendOutput(output, typeof res === 'string' ? res : JSON.stringify(res), '<<<');
    }).catch(e => appendOutput(output, e.message, 'ERR'));
  }

  function quickButton(label, cmd) {
    return h('button', { class: 'btn', onClick: () => { input.value = cmd; sendCommand(); } }, label);
  }

  function renderSaved() {
    clear(saved);
    loadSaved().forEach(item => {
      saved.appendChild(h('span', { class: 'saved-item' }, [
        quickButton(item.label, item.command),
        h('button', { class: 'btn', title: '删除已保存命令', onClick: () => { localStorage.setItem(SAVED_KEY, JSON.stringify(loadSaved().filter(e => e.command !== item.command))); renderSaved(); } }, '×')
      ]));
    });
    saved.style.display = loadSaved().length ? '' : 'none';
  }

  function saveCommand() {
    const cmd = (input.value || '').trim();
    if (!cmd) { UI.toast('命令为空', 'warning'); return; }
    const label = prompt('为命令命名', cmd);
    if (!label) return;
    const list = loadSaved().filter(i => i.command !== cmd);
    list.push({ label: label.substring(0, 40), command: cmd });
    localStorage.setItem(SAVED_KEY, JSON.stringify(list.slice(-20)));
    renderSaved();
  }

  input.addEventListener('keydown', ev => { if (ev.key === 'Enter') sendCommand(); });
  setTimeout(renderSaved, 0);

  container.appendChild(h('div', {}, [
    h('div', { class: 'hero' }, [
      h('div', {}, [h('h2', {}, 'MT5700M AT 命令控制台'), h('div', { class: 'hero-sub' }, '面向高级用户的诊断控制台。命令直接发送到 MT5700M，不会自动校验。')])
    ]),
    h('section', { class: 'ui-card control-card' }, [
      h('div', { class: 'alert warning' }, '尽量使用查询命令。配置与复位命令可能中断移动连接。'),
      h('div', { class: 'terminal-row' }, [
        input,
        h('button', { class: 'btn primary', onClick: sendCommand }, '发送'),
        h('button', { class: 'btn', onClick: () => { output.textContent = ''; } }, '清空'),
        h('button', { class: 'btn', onClick: saveCommand }, '保存命令')
      ]),
      h('div', { class: 'quick' }, [
        quickButton('AT', 'AT'), quickButton('ATI', 'ATI'), quickButton('SIM', 'AT+CPIN?'),
        quickButton('信号', 'AT^HCSQ?'), quickButton('温度', 'AT^CHIPTEMP?'), quickButton('运营商', 'AT+COPS?'),
        quickButton('小区信息', 'AT^MONSC'), quickButton('NR 锁频', 'AT^NRFREQLOCK?'), quickButton('LTE 锁频', 'AT^LTEFREQLOCK?')
      ]),
      saved,
      output
    ])
  ]));
});
