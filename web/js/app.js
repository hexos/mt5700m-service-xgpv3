'use strict';
/* 路由 + 导航 + 全局状态 + 连接管理 */

const NAV = [
  { label: '网络', items: [
    { id: 'status', title: '概览' },
    { id: 'connection', title: '移动数据' },
    { id: 'network', title: '无线与小区' },
  ]},
  { label: '模组', items: [
    { id: 'system', title: '模组与 SIM' },
    { id: 'sms', title: '短信' },
  ]},
  { label: '工具', items: [
    { id: 'advanced', title: '高级设置' },
    { id: 'settings', title: '通信诊断' },
    { id: 'terminal', title: 'AT 控制台' },
  ]},
];

function currentRoute() {
  const hash = (location.hash || '').replace(/^#\/?/, '');
  const id = hash.split('/')[0] || 'status';
  return window.PageRegistry[id] ? id : 'status';
}

function render() {
  const id = currentRoute();
  document.querySelectorAll('.nav-item').forEach(b => b.classList.toggle('active', b.dataset.id === id));
  const root = document.getElementById('page-root');
  clear(root);
  const page = h('div', { class: 'page' });
  root.appendChild(page);
  try {
    window.PageRegistry[id](page);
  } catch (e) {
    console.error(e);
    page.replaceChildren(h('div', { class: 'read-error' }, '页面加载失败: ' + (e.message || e)));
  }
}
function asyncPage(container, load, render) {
  let busy = false;
  function refresh() {
    if (busy) return;
    busy = true;
    container.replaceChildren(UI.spin());
    Promise.resolve().then(load).then(data => {
      if (container.isConnected) container.replaceChildren(render(data));
    }).catch(e => {
      if (container.isConnected) container.replaceChildren(h('div', { class: 'read-error' }, [
        h('p', {}, e.message || '读取失败，设备状态未知'),
        h('button', { class: 'btn', onClick: refresh }, '重试')
      ]));
    }).then(() => { busy = false; });
  }
  refresh();
  return { refresh };
}
window.asyncPage = asyncPage;

function buildNav() {
  const nav = document.getElementById('nav');
  clear(nav);
  NAV.forEach(group => {
    const g = h('div', { class: 'nav-group' }, [h('div', { class: 'nav-group-label' }, group.label)]);
    group.items.forEach(item => {
      g.appendChild(h('button', { class: 'nav-item', dataset: { id: item.id }, onClick: () => { location.hash = '#/' + item.id; } }, item.title));
    });
    nav.appendChild(g);
  });
}

function bindStatus() {
  const badge = document.getElementById('conn-badge');
  Api.onStatus(state => {
    badge.dataset.state = state;
    if (state === 'online') badge.textContent = '已连接';
    else if (state === 'connecting') badge.textContent = '连接中…';
    else if (state === 'auth-failed') badge.textContent = '认证失败';
    else if (state === 'error') badge.textContent = '连接错误';
    else badge.textContent = '连接断开';
  });
}

function start() {
  buildNav();
  bindStatus();
  const wsPort = localStorage.getItem('mt5700m.wsport') || '8766';
  const auth = localStorage.getItem('mt5700m.auth') || '';
  const url = (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.hostname + ':' + wsPort;
  Api.connect(url, auth);
  window.addEventListener('hashchange', render);
  render();
}

document.addEventListener('DOMContentLoaded', start);
