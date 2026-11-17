'use strict';
'require view';

/* MT5700M 面板入口：
 * 前端静态资源随 mt5700m-service 安装到 /www/mt5700m/，
 * 由 uhttpd 直接提供；WebSocket 仍连 8766 端口。
 */
return view.extend({
	render: function () {
		var host = window.location.hostname;
		var direct = window.location.protocol + '//' + host + ':8088/';

		return E('div', { class: 'cbi-map' }, [
			E('h2', {}, _('MT5700M 模组')),
			E('div', { class: 'cbi-map-descr' }, [
				_('AT 通道经 QModem 的 at-daemon 共享，不会与 QModem 抢串口。'),
				E('br'),
				_('独立页面：'),
				E('a', { href: direct, target: '_blank', rel: 'noopener' }, direct)
			]),
			E('iframe', {
				src: '/mt5700m/',
				style: 'width:100%;height:calc(100vh - 190px);min-height:560px;border:0;border-radius:8px;background:#0b0f14',
				title: 'MT5700M'
			})
		]);
	},

	handleSaveApply: null,
	handleSave: null,
	handleReset: null
});
