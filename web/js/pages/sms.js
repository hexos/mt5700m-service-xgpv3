'use strict';
/* 短信页：会话 + 聊天 + 发送/删除 + 设置 */

window.registerPage('sms', function (container) {
  const { U, UI } = window;
  const SENT_KEY = 'mt5700m.sms.sent';
  let received = [], info = null, groups = [], selected = null, writing = false, validList = false;

  function sentHistory() {
    try { return JSON.parse(localStorage.getItem(SENT_KEY) || '[]').filter(i => i && i.number && i.text).map(i => Object.assign({ direction: 'out', indexes: [] }, i)); } catch (e) { return []; }
  }
  function saveSent(number, text) {
    const now = new Date();
    const hist = sentHistory();
    hist.push({ number, text, date: `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')} ${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`, order: now.getTime() });
    localStorage.setItem(SENT_KEY, JSON.stringify(hist.slice(-500)));
  }

  function settingsModal(info) {
    const smsc = h('input', { class: 'input', value: info.smsc || '' });
    const storage = UI.select([['SM', 'SIM 卡'], ['ME', '模组存储']], info.storage[0] || 'SM');
    const ims = UI.select([['1', '启用'], ['0', '禁用']], info.ims === '0' ? '0' : '1');
    const m = UI.modal('短信设置', h('div', {}, [
      UI.row('短信服务 (IMS)', ims),
      h('div', { class: 'alert warning' }, '更改 IMS 短信服务会切换飞行模式。若运营商不支持 IMS 短信请保持启用。'),
      UI.row('短消息中心', smsc),
      UI.row('存储位置', storage)
    ]), [
      h('button', { class: 'btn default', onClick: m.close }, '关闭'),
      h('button', { class: 'btn primary', onClick: async () => {
        const v = smsc.value.trim();
        if (!/^\+?[0-9]{5,20}$/.test(v)) { UI.toast('请输入有效的短消息中心号码', 'warning'); return; }
        m.close();
        const changes = [];
        if (v !== info.smsc) changes.push(['smsc', () => Api.call('sms-set', { key: 'smsc', value: v })]);
        if (storage.value !== info.storage[0]) changes.push(['storage', () => Api.call('sms-set', { key: 'storage', value: storage.value })]);
        if (ims.value !== info.ims) changes.push(['ims', () => Api.call('sms-ims', { state: ims.value })]);
        if (!changes.length) return;
        const doApply = async () => { for (const [, fn] of changes) await fn(); UI.toast('命令已接受，正在回读设置', 'success'); };
        if (ims.value !== info.ims) { if (await UI.confirm('更改 IMS 短信服务？', '此操作会切换飞行模式并短暂中断蜂窝网络。', { danger: true })) await doApply(); }
        else await doApply();
        refresh();
      } }, '保存设置')
    ]);
  }

  function build() {
    container.replaceChildren();
    const status = h('div', { class: 'sms-status' }, '正在读取短信…');
    const storage = h('div', { class: 'sms-storage' }, '存储状态待确认');
    const contacts = h('div');
    const thread = h('div', { class: 'sms-thread' });
    const title = h('strong', {}, '新短信');
    const recipient = h('input', { class: 'input', placeholder: '收件号码', autocomplete: 'off' });
    const recipientWrap = h('div', { class: 'sms-recipient show' }, recipient);
    const text = h('textarea', { class: 'input', rows: '2', maxlength: '500', placeholder: '输入短信内容…' });
    const count = h('span', {}, '—');
    const sendButton = h('button', { class: 'btn primary', onClick: send }, '发送');
    const refreshButton = h('button', { class: 'btn', onClick: () => refresh() }, '刷新');
    const settingsButton = h('button', { class: 'btn', disabled: true, onClick: () => { if (info && !writing) settingsModal(info); } }, '设置');

    function err(e) { status.className = 'sms-status'; status.textContent = e.message || '操作失败，请重试。'; }
    function confirmBox(title, msg, action) { UI.confirm(title, msg, { danger: true }).then(ok => { if (ok) action(); }); }

    function show(number, preserve) {
      selected = number;
      const group = groups.filter(g => g.number === number)[0];
      if (number !== null) recipient.value = number;
      recipientWrap.classList.toggle('show', number === null);
      title.textContent = number || '新短信';
      contacts.querySelectorAll('.sms-contact').forEach(n => n.classList.toggle('active', n.dataset.number === number));
      thread.replaceChildren(...(group ? group.messages.map(msg => h('div', { class: 'bubblewrap ' + (msg.direction === 'out' ? 'out' : 'in') }, [
        h('div', { class: 'bubble' }, [msg.text || '空短信', h('div', { class: 'bubbledate' }, msg.date)]),
        h('button', { class: 'sms-delete', title: '删除此条', disabled: writing, onClick: () => confirmBox('删除此条短信？', msg.direction === 'out' ? '仅删除此浏览器的发送记录。' : '将删除模组内这条短信，不可撤销。', () => {
          if (writing) return; writing = true;
          const job = msg.direction === 'out' ? Promise.resolve(localStorage.setItem(SENT_KEY, JSON.stringify(sentHistory().filter(i => i.order !== msg.order))))
            : (msg.indexes || []).reduce((p, idx) => p.then(() => Api.call('sms-delete', { index: idx })), Promise.resolve());
          job.then(() => refresh()).catch(err).then(() => { writing = false; });
        }) }, '×')
      ])) : [h('div', { class: 'sms-empty' }, validList ? '选择会话，或输入号码编写新短信。' : '短信读取尚未成功；这不代表收件箱为空。')]));
    }

    function redraw() {
      groups = U.groupMessages(received.concat(sentHistory()));
      count.textContent = validList ? String(groups.length) : '—';
      contacts.replaceChildren(...groups.map(g => {
        const last = g.messages[g.messages.length - 1];
        return h('button', { class: 'sms-contact', dataset: { number: g.number }, onClick: () => show(g.number, false) }, [
          h('div', { class: 'sms-contact-top' }, [h('span', { class: 'sms-contact-number' }, g.number), h('span', { class: 'sms-contact-date' }, String(last.date || '').substring(5))]),
          h('div', { class: 'sms-contact-preview' }, last.text || '空短信')
        ]);
      }));
      show(selected, true);
    }

    function refresh() {
      refreshButton.disabled = true;
      const errors = [];
      return Api.call('sms-list').then(r => {
        received = U.parseMessages(r.messages || '');
        validList = true;
        return Api.call('sms-info').then(r2 => {
          const current = U.parseInfo(U.labeledToRaw(r2));
          if (!current.storage.length || !current.smsc || !/^[01]$/.test(current.ims)) throw new Error('短信设置不完整');
          info = current;
          storage.textContent = `已用 ${info.storage[1]} / ${info.storage[2]} 条`;
        }).catch(() => { info = null; storage.textContent = '短信设置读取失败'; errors.push('设置暂不可用'); });
      }).then(() => {
        if (container.isConnected) { redraw(); status.textContent = errors.join('；'); settingsButton.disabled = !info; }
      }).catch(err).then(() => { refreshButton.disabled = false; });
    }

    function send() {
      if (writing) return;
      const number = (selected || recipient.value).trim(), body = text.value.trim();
      if (!/^\+?[0-9]{5,20}$/.test(number) || !body) { err(Error('请输入有效收件号码和内容。')); return; }
      confirmBox('发送短信？', `确认向 ${number} 发送，可能产生运营商费用。`, () => {
        writing = true; sendButton.disabled = true;
        Api.call('sms-send', { number, text: body }).then(() => {
          text.value = ''; selected = number; saveSent(number, body);
          UI.toast('模组已确认提交；不代表对方已收到', 'success');
          return refresh();
        }).catch(err).then(() => { writing = false; sendButton.disabled = false; });
      });
    }

    container.appendChild(h('div', {}, [
      h('div', { class: 'hero' }, [
        h('div', {}, [h('h2', {}, '短信'), h('div', { class: 'hero-sub' }, storage)]),
        h('div', { class: 'sms-hero-actions' }, [
          h('button', { class: 'btn ghost', onClick: () => show(null, false) }, '新短信'), refreshButton, settingsButton
        ])
      ]),
      status,
      h('div', { class: 'sms-shell' }, [
        h('aside', { class: 'sms-sidebar' }, [h('div', { class: 'sms-sidehead' }, [h('strong', {}, '会话'), count]), contacts]),
        h('section', { class: 'sms-chat' }, [h('div', { class: 'sms-chathead' }, title), thread, h('div', { class: 'sms-compose' }, [recipientWrap, h('div', { class: 'sms-compose-row' }, [text, sendButton])])])
      ])
    ]));
    setTimeout(refresh, 0);
  }

  build();
});
