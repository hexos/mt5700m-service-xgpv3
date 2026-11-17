# mt5700m-service

把 [luci-app-mt5700m](https://github.com/FAN789/luci-app-mt5700m) 的整套后端逻辑
（Shell rpcd + C 传输）用 **Rust** 重写而成的独立服务，协议上做成
[mt5700webui-openwrt-server](https://github.com/hexos/mt5700webui-openwrt-server)
`at-webserver` WebSocket 协议的**超集**，可直接与其前端/同一套 WebSocket 客户端一起使用。

> 目标：在 OpenWrt 上，用一个 Rust 二进制替代 `mt5700m-at`、`mt5700m-read`、
> `mt5700m-manager`、`mt5700m-signal`、`mt5700m-traffic`、`mt5700m-transport`
> 以及 `rpcd/mt5700m` 这一串脚本/C 程序，提供同样的 MT5700M 管理能力。

## 功能

| 类别 | 说明 |
| --- | --- |
| 连接方式 | 串口直连（PCUI AT 端口）/ 网络 TCP（192.168.8.1:20249），`mode=auto` 自动探测 |
| 状态查询 | 身份(ATI/IMEI)、SIM/运营商、本机号码、信号(HCSQ)、签约速率、载波聚合、温度、锁频状态 |
| 网络/系统/高级 | 分组原始 AT 结果（connection / session / radio / hardware 等） |
| SMS | 列表(PDU)、发送(UCS2 多段)、删除、清空、SMSC/存储设置 |
| 锁频 | LTE/NR 频段/频点/小区锁，含输入校验与 CFUN 切换校验 |
| 连接管理 | connect / disconnect / redial / sync（network UCI 同步 + NDIS 会话） |
| 其它写操作 | 飞行模式、重启、无线制式、自动拨号、NIC 速率、LED、USB 模式、PDP、SIM PIN、IMS、FOTA、温控等 |
| 后台采集 | 信号（`AT^HCSQ?` 缓存）、流量（rx/tx 字节差累计总/日/月） |

## WebSocket 协议（`at-webserver` 超集）

- 心跳：客户端发纯文本 `ping`，服务端回 `pong`。
- 原生 AT 透传：客户端发纯文本 `AT+CMGF?`，服务端回
  `{"success":true,"data":"...","error":null}`（与 at-webserver 完全一致）。
- 结构化命令：客户端发 JSON `{"action":"status"}`，服务端回结构化 JSON。
- URC 主动上报：`{"type":"raw_data","data":"+CMTI: ..."}`。
- 可选认证：配置 `auth_key` 后先发 `{"auth_key":"..."}`。

常用结构化 action：

```
status, signal, temperature, network, system, advanced{group}, sms-list, sms-info,
traffic, connection-status, log, at{command},
connect, disconnect, redial, sync,
lock{rat,type,bands,arfcns,scs,pcis}, unlock, airplane{state}, restart,
radio-mode{value}, radio-policy{...}, 5g-access{preset}, autodial{...},
nic-speed, pcie-controller, led, usb-mode, interface-mode, postroute, dmz,
sim-hotplug, sim-activation, sim-slot, thermal, thermal-log, thermal-thresholds,
carrier-aggregation, vonr, dss, pdp-set, pdp-remove, pdp-state, ndis, flow-clear,
sim-pin{operation,a,b}, sms-ims{state}, factory-reset, set-imei{imei},
fota-init, fota-state, fota-progress, fota-download{url}, fota-start{url},
fota-resume, fota-upgrade, sms-send{number,text}, sms-delete{index},
sms-clear, sms-set{key,value}
```

示例：

```bash
# 用 websocat / wscat 连接
ws://路由器IP:8766

# 结构化状态
{"action":"status"}

# 发送短信
{"action":"sms-send","number":"10086","text":"你好"}

# NR 频段锁（type=3：仅频段）
{"action":"lock","rat":"nr","type":3,"bands":"78,79"}
```

## HTTP 端点（只读）

- `GET /api/status`、`/api/signal`、`/api/temperature`、`/api/network`
- `GET /api/system`、`/api/traffic`、`/api/connection-status`、`/api/log`

## Web UI（内置前端）

`web/` 目录内置了一个**独立静态前端**（纯 HTML/CSS/JS，不依赖 LuCI），
完整移植了 luci-app-mt5700m 的 8 个页面，通过 WebSocket 对接本服务：

| 路由 | 页面 |
| --- | --- |
| `#/status` | 概览（信号 / 载波聚合 / 接口 / 流量） |
| `#/connection` | 移动数据（拨号 / 会话 / PDP / 配置） |
| `#/network` | 无线与小区（服务小区 / 诊断 / 锁频 / 无线偏好） |
| `#/system` | 模组与 SIM（身份 / 运行状态 / 温度 / FOTA） |
| `#/sms` | 短信（会话 / 发送 / 设置） |
| `#/advanced` | 高级设置（硬件接口 / SIM / 温度） |
| `#/settings` | 通信诊断（AT 通道配置） |
| `#/terminal` | AT 控制台 |

服务启动后，浏览器直接访问：

```
http://路由器IP:8088/
```

前端由 `mt5700m-service` 的 HTTP 服务直接托管（无需额外 Web 服务器），
WebSocket 默认连接同主机的 `8766` 端口；可通过 `localStorage.mt5700m.wsport`
或 URL 参数 `?wsport=` 覆盖。

## 配置（UCI）

```bash
# 连接方式 auto/serial/network
uci set mt5700m.settings.mode='auto'
uci set mt5700m.settings.host='192.168.8.1'
uci set mt5700m.settings.port='20249'

# 拨号配置
uci set mt5700m.connection.apn=''
uci set mt5700m.connection.pdp_type='ipv4v6'
uci set mt5700m.connection.auth='none'

# API
uci set mt5700m.api.websocket_port='8766'
uci set mt5700m.api.http_port='8088'
uci set mt5700m.api.auth_key=''

uci commit mt5700m
/etc/init.d/mt5700m-service restart
```

## 编译

本机（需要 Rust 工具链）：

```bash
cargo build --release
cargo test
```

OpenWrt 交叉编译：把本仓库放到 OpenWrt 源码树的 `package/` 下，然后：

```bash
make package/mt5700m-service/compile V=s
```

## 与 mt5700webui-openwrt-server 的关系

- `at-webserver`：负责「原始 AT 透传 + netmon + 通知 + 飞行模式」，WebSocket 端口默认 `8765`。
- `mt5700m-service`：负责「结构化 MT5700M 管理 API」，WebSocket 端口默认 `8766`，
  提供 luci-app-mt5700m 的完整业务逻辑。两者可同时运行，也可以只用其一。

本服务的 WebSocket 协议是 at-webserver 的**超集**：同一个前端 WebSocket 客户端
既能发原生 AT 命令，也能发结构化 `{"action":...}` 命令。

## License

Apache-2.0
