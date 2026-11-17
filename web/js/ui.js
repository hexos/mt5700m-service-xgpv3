'use strict';
/* 通用 UI 组件，构建于 h() 之上。 */

const UI = {
  /* 右下角通知 */
  toast(message, type = 'info') {
    const wrap = document.getElementById('toast-wrap');
    const el = h('div', { class: 'toast ' + type }, message);
    wrap.appendChild(el);
    setTimeout(() => { el.classList.add('show'); }, 10);
    setTimeout(() => { el.classList.remove('show'); setTimeout(() => el.remove(), 300); }, 3600);
  },

  /* 模态框 */
  modal(title, node, actions = []) {
    const wrap = document.getElementById('modal-wrap');
    const overlay = h('div', { class: 'modal-overlay', onClick: (e) => { if (e.target === overlay) close(); } });
    const close = () => { overlay.remove(); };
    const box = h('div', { class: 'modal-box' }, [
      h('div', { class: 'modal-head' }, [h('div', { class: 'modal-title' }, title), h('button', { class: 'modal-x', onClick: close }, '×')]),
      h('div', { class: 'modal-body' }, node),
      actions.length ? h('div', { class: 'modal-actions' }, actions) : null
    ]);
    overlay.appendChild(box);
    wrap.appendChild(overlay);
    return { close, box, overlay };
  },

  /* 确认框，返回 Promise<boolean> */
  confirm(title, message, opts = {}) {
    return new Promise((resolve) => {
      let done = false;
      const finish = (v) => { if (done) return; done = true; m.close(); resolve(v); };
      const m = UI.modal(title, h('p', { class: 'modal-msg' }, message), [
        h('button', { class: 'btn default', onClick: () => finish(false) }, opts.cancelText || '取消'),
        h('button', { class: 'btn ' + (opts.danger ? 'danger' : 'primary'), onClick: () => finish(true) }, opts.confirmText || '继续')
      ]);
    });
  },

  /* 执行确认的写操作 */
  async confirmRun(actionCall, title, message, opts = {}) {
    const ok = await UI.confirm(title, message, opts);
    if (!ok) return false;
    UI.toast('正在应用…', 'info');
    try {
      await actionCall();
      UI.toast(opts.successText || '命令已接受', 'success');
      return true;
    } catch (e) {
      UI.toast(e.message || '操作失败', 'error');
      return false;
    }
  },

  /* 卡片 */
  card(title, desc, body, wide = false) {
    return h('section', { class: 'ui-card control-card' + (wide ? ' wide' : '') }, [
      h('h3', {}, title),
      desc ? h('div', { class: 'control-desc' }, desc) : null,
      h('div', { class: 'control-body' }, body)
    ]);
  },

  /* 表单行 */
  row(label, control) {
    return h('div', { class: 'control-row' }, [h('label', {}, label), control]);
  },

  /* 只读状态行 */
  stateRow(label, value) {
    return h('div', { class: 'control-state' }, [h('span', {}, label), h('strong', {}, value || '--')]);
  },

  /* 按钮 */
  actionBtn(label, handler, kind = 'primary') {
    return h('div', { class: 'control-actions' }, h('button', { class: 'btn ' + kind, onClick: handler }, label));
  },

  /* 下拉选择 */
  select(options, value = null) {
    const node = h('select', { class: 'input' }, options.map(o => h('option', { value: o[0] }, o[1])));
    if (value != null) node.value = String(value);
    return node;
  },

  /* 徽章 */
  badge(text, cls = '') {
    return h('span', { class: 'badge ' + cls }, text);
  },

  /* 折叠区 */
  details(summaryTitle, summaryDesc, body) {
    const bodyBox = h('div', { class: 'details-body' }, body);
    return h('details', { class: 'ui-details' }, [
      h('summary', {}, [
        h('span', { class: 'summary-copy' }, [h('span', { class: 'summary-title' }, summaryTitle), summaryDesc ? h('span', { class: 'summary-desc' }, summaryDesc) : null]),
        h('span', { class: 'chevron' }, '›')
      ]),
      bodyBox
    ]);
  },

  /* 加载中 */
  spin(text = '正在读取…') {
    return h('div', { class: 'spin' }, [h('span', { class: 'spinner' }), h('p', {}, text)]);
  },

  /* hero 头部 */
  hero(kicker, title, sub, right) {
    return h('section', { class: 'hero' }, [
      h('div', {}, [kicker ? h('div', { class: 'hero-kicker' }, kicker) : null, h('h2', {}, title), sub ? h('div', { class: 'hero-sub' }, sub) : null]),
      right ? h('div', { class: 'hero-side' }, right) : null
    ]);
  },

  /* 原始输出块 */
  rawBlock(text, cls = 'raw') {
    return h('pre', { class: cls }, text || '无响应');
  },

  /* 数据行 */
  kv(label, value) {
    return h('div', { class: 'kv-row' }, [h('span', {}, label), h('strong', {}, value || '--')]);
  },

  /* 指标卡 */
  metric(label, value, unit) {
    return h('div', { class: 'metric' }, [
      h('div', { class: 'metric-label' }, label),
      h('span', { class: 'metric-value' }, value || '--'),
      value ? h('span', { class: 'metric-unit' }, unit) : null
    ]);
  }
};

window.UI = UI;
