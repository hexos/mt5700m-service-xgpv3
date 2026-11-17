'use strict';
/* 轻量 DOM 构建工具，替代 LuCI 的 E()。 */

function h(tag, attrs, children) {
  let id = null, classes = [];
  const hash = tag.indexOf('#');
  if (hash >= 0) { id = tag.slice(hash + 1); tag = tag.slice(0, hash); }
  const dot = tag.indexOf('.');
  if (dot >= 0) { classes = tag.slice(dot + 1).split('.'); tag = tag.slice(0, dot); }

  const el = document.createElement(tag || 'div');
  if (id) el.id = id;
  if (classes.length) el.classList.add(...classes.filter(Boolean));

  if (attrs) {
    for (const [key, val] of Object.entries(attrs)) {
      if (val == null || val === false) continue;
      if (key === 'class') {
        String(val).split(/\s+/).filter(Boolean).forEach(c => el.classList.add(c));
      } else if (key === 'style') {
        if (typeof val === 'object') Object.assign(el.style, val);
        else el.style.cssText = val;
      } else if (key.startsWith('on') && typeof val === 'function') {
        el.addEventListener(key.slice(2).toLowerCase(), val);
      } else if (key === 'dataset') {
        Object.assign(el.dataset, val);
      } else if (key === 'html') {
        el.innerHTML = val;
      } else if (key in el && key !== 'list' && key !== 'type') {
        // 属性优先（value/checked/disabled/selected 等）
        try { el[key] = val; } catch (e) { el.setAttribute(key, val); }
      } else {
        if (val === true) el.setAttribute(key, '');
        else el.setAttribute(key, val);
      }
    }
  }

  appendChildren(el, children);
  return el;
}

function appendChildren(el, children) {
  if (children == null || children === false) return;
  if (Array.isArray(children)) {
    children.forEach(c => appendChildren(el, c));
  } else if (children instanceof Node) {
    el.appendChild(children);
  } else {
    el.appendChild(document.createTextNode(String(children)));
  }
}

function clear(el) { while (el.firstChild) el.removeChild(el.firstChild); return el; }

window.h = h;
window.clear = clear;

/* 页面注册表（须在页面模块之前定义） */
window.PageRegistry = {};
window.registerPage = function (id, render) { window.PageRegistry[id] = render; };
