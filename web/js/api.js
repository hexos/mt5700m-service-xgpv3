'use strict';
/* WebSocket 客户端：串行请求队列，对接 mt5700m-service 的 WS 协议。 */

class ApiClient {
  constructor() {
    this.ws = null;
    this.connected = false;
    this.authKey = '';
    this.queue = [];
    this.busy = false;
    this._pending = null;
    this._urcHandlers = [];
    this._statusHandlers = [];
    this._ready = false;
    this._authResolvers = [];
    this._heartbeat = null;
  }

  onStatus(fn) { this._statusHandlers.push(fn); }
  onUrc(fn) { this._urcHandlers.push(fn); }
  _emitStatus(state) { this._statusHandlers.forEach(fn => { try { fn(state); } catch (e) {} }); }

  connect(url, authKey) {
    this.authKey = authKey || '';
    this._ready = this.authKey ? false : true;
    this._emitStatus('connecting');
    try {
      this.ws = new WebSocket(url);
    } catch (e) {
      this._emitStatus('error');
      return;
    }
    this.ws.onopen = () => {
      this.connected = true;
      if (this.authKey) {
        this._sendRaw(JSON.stringify({ auth_key: this.authKey }));
      } else {
        this._emitStatus('online');
        this._drain();
      }
      this._startHeartbeat();
    };
    this.ws.onmessage = (evt) => this._onMessage(evt.data);
    this.ws.onclose = () => { this._teardown(); this._emitStatus('offline'); setTimeout(() => this.connect(url, this.authKey), 2000); };
    this.ws.onerror = () => { this._teardown(); this._emitStatus('error'); };
  }

  _teardown() {
    this.connected = false;
    if (this._heartbeat) { clearInterval(this._heartbeat); this._heartbeat = null; }
    if (this._pending) { const p = this._pending; this._pending = null; this.busy = false; p.reject(new Error('连接已断开')); }
    this._drain();
  }

  _startHeartbeat() {
    if (this._heartbeat) clearInterval(this._heartbeat);
    this._heartbeat = setInterval(() => { if (this.connected) this._sendRaw('ping'); }, 25000);
  }

  _sendRaw(text) { try { this.ws.send(text); } catch (e) {} }

  _onMessage(data) {
    if (typeof data !== 'string') return;
    if (data === 'pong') return;
    let msg;
    try { msg = JSON.parse(data); } catch (e) { return; }
    if (msg.type === 'raw_data') { this._urcHandlers.forEach(fn => { try { fn(msg.data); } catch (e) {} }); return; }
    // 认证响应
    if (msg.message === '认证成功' || (msg.success === true && !('data' in msg) && msg.error == null && msg.message)) {
      this._ready = true;
      this._emitStatus('online');
      this._drain();
      return;
    }
    if (msg.error === 'Authentication failed') {
      this._ready = false;
      this._emitStatus('auth-failed');
      return;
    }
    // 普通请求响应
    if (this._pending) {
      const p = this._pending; this._pending = null; this.busy = false;
      if (msg.success) p.resolve(msg.data); else p.reject(new Error(msg.error || '操作失败'));
      this._drain();
    }
  }

  _drain() {
    if (this.busy || !this.connected || !this._ready || this.queue.length === 0) return;
    this.busy = true;
    const item = this.queue.shift();
    this._pending = { resolve: item.resolve, reject: item.reject };
    if (item.raw) this._sendRaw(item.payload);
    else this._sendRaw(JSON.stringify(item.payload));
  }

  _enqueue(item) {
    return new Promise((resolve, reject) => {
      this.queue.push({ ...item, resolve, reject });
      this._drain();
    });
  }

  /* 结构化命令：call('status') / call('lock', {rat:'nr',...}) */
  call(action, params = {}) {
    const payload = { action, ...params };
    return this._enqueue({ payload });
  }

  /* 原生 AT 命令透传 */
  at(command) {
    return this._enqueue({ raw: true, payload: command }).then(data => typeof data === 'string' ? data : JSON.stringify(data));
  }
}

window.Api = new ApiClient();
