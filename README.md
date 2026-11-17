# mt5700m-service（XGPV3 / ImmortalWrt 适配版）

把 [luci-app-mt5700m](https://github.com/FAN789/luci-app-mt5700m) 的整套后端逻辑
（Shell rpcd + C 传输）用 **Rust** 重写而成的独立服务，协议上是
[mt5700webui-openwrt-server](https://github.com/hexos/mt5700webui-openwrt-server)
`at-webserver` WebSocket 协议的**超集**。

本仓库在原版基础上做了 **XGPV3（西瓜皮 V3）适配**，关键点只有一个：
**AT 串口不再由本服务独占，改为经 QModem 的 `at-daemon`（ubus）共享。**
XGPV3 上 QModem 已经 `open()` 了模组的 AT 口，任何再自己 `open()` 的程序都会
读走 QModem 的响应、造成双方超时——这也是之前同类面板被移出固件的原因。

## XGPV3 数据通路

```
LuCI /mt5700m/ 或 :8088  ──WebSocket(8766)──>  mt5700m-service (Rust)
                                                      │
                          ┌───────────────────────────┴──────────────────────────┐
                          │  AT 命令：ubus call at-daemon sendat                  │  URC：ubus listen
                          ▼                                                      ▼
                    at-daemon（QModem 持有 /dev/ttyUSBx）        qmodem.at.line / qmodem.at.urc
                          │
                          ▼
                     MT5700M 模组（USB / PCIe 转接板 / MHI）
```

- **不抢串口**：`mode=ubus` 时本服务从不 `open()` 串口，只调用 at-daemon。
- **端口自动识别**：`at_port=auto` 时取 `ubus call at-daemon list` 中 `is_open=1`
  的端口；at-daemon 未就绪会等约 15 秒，再退回 `/dev` 节点扫描，最后用
  `at_port_fallback`（XGPV3 实测 `/dev/ttyUSB1`）。模组重插后自动跟换，无需重启。
- **URC 不丢**：订阅 `qmodem.at.line` / `qmodem.at.urc` 并按
  `(restart_epoch, sequence)` 去重，短信/来电 URC 照常推给前端。
- **不重复初始化**：UBUS 模式下默认不发送 `ATE0`/`AT+CNMI`，避免改动 QModem 依赖的
  URC 配置（需要时设 `mt5700m.settings.force_init=1`）。
- **不抢拨号**：`connection.managed=qmodem`（默认）时，连接/断开/重拨只返回提示，
  不改 `network` UCI、不发 `AT^NDISDUP`，网卡完全交给 QModem。

## 功能

| 类别 | 说明 |
| --- | --- |
| 连接方式 | `auto` / `ubus` / `serial` / `network` / `tom_modem`，`auto` 优先 ubus |
| 状态查询 | 身份(ATI/IMEI)、SIM/运营商、本机号码、信号(HCSQ)、签约速率、载波聚合、温度、锁频状态 |
| 网络/系统/高级 | 分组原始 AT 结果（connection / session / radio / hardware 等） |
| SMS | 列表(PDU)、发送(UCS2 多段)、删除、清空、SMSC/存储设置 |
| 锁频 | LTE/NR 频段/频点/小区锁，含输入校验与 CFUN 切换校验 |
| 连接管理 | connect / disconnect / redial / sync（`managed=self` 时才真正操作网卡） |
| 其它写操作 | 飞行模式、重启、无线制式、自动拨号、NIC 速率、LED、USB 模式、PDP、SIM PIN、IMS、FOTA、温控等 |
| 后台采集 | 信号（`AT^HCSQ?` 缓存）、流量（rx/tx 字节差累计总/日/月） |

## 安装

### 随固件编译（推荐）

本仓库是标准 OpenWrt 包目录（仓库根目录即 `mt5700m-service` 包），
`luci-app-mt5700m/` 是配套的 LuCI 入口包。放到源码树后：

```bash
ln -s /path/to/mt5700m-service package/zz/mt5700m-service   # 或用构建仓库的 prepare.sh
cp -r luci-app-mt5700m package/zz/
make menuconfig     # Network -> mt5700m-service；LuCI -> Applications -> luci-app-mt5700m
make package/mt5700m-service/compile V=s
```

完整的 XGPV3 自动构建见
[hexos/immortalwrt-xgp-auto-build-mt5700m](https://github.com/hexos/immortalwrt-xgp-auto-build-mt5700m)，
`prepare.sh` 会自动拉取本仓库并选中相关包。

### 手动安装 ipk

```sh
opkg install mt5700m-service_*.ipk luci-app-mt5700m_*.ipk
/etc/init.d/mt5700m-service enable
/etc/init.d/mt5700m-service start
```

## 使用

- LuCI：`网络 → MT5700M 模组`（内嵌 `/mt5700m/`）
- 浏览器：`http://<路由>/mt5700m/`（uhttpd 静态页）或 `http://<路由>:8088/`（服务自带）
- WebSocket API：`ws://<路由>:8766`
- HTTP 只读：`/api/status`、`/api/signal`、`/api/temperature`、`/api/network`、
  `/api/system`、`/api/traffic`、`/api/connection-status`、`/api/log`、`/api/config`

## 配置（UCI）

```sh
uci set mt5700m.settings.mode='ubus'              # auto/ubus/serial/network/tom_modem
uci set mt5700m.settings.at_port='auto'           # auto 或 /dev/ttyUSBx
uci set mt5700m.settings.at_port_fallback='/dev/ttyUSB1'
uci set mt5700m.settings.timeout='8'
uci set mt5700m.settings.force_init='0'           # UBUS 模式下是否重发 ATE0/CNMI

uci set mt5700m.connection.managed='qmodem'       # qmodem=只做面板，self=自己拨号
uci set mt5700m.connection.netdev=''              # 留空自动识别数据网卡

uci set mt5700m.api.websocket_port='8766'
uci set mt5700m.api.http_port='8088'
uci set mt5700m.api.auth_key=''                   # 非空时 WS 先发 {"auth_key":"..."}

uci commit mt5700m
/etc/init.d/mt5700m-service restart
```

## WebSocket 协议

- 心跳：客户端发纯文本 `ping`，服务端回 `pong`。
- 原生 AT 透传：发纯文本 `AT+CSQ`，回 `{"success":true,"data":"...","error":null}`。
- 结构化命令：发 `{"action":"status"}`。
- URC 主动上报：`{"type":"raw_data","data":"+CMTI: ..."}`。
- 可选认证：配置 `auth_key` 后先发 `{"auth_key":"..."}`。

常用 action：

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
websocat ws://192.168.1.1:8766
{"action":"status"}
{"action":"sms-send","number":"10086","text":"你好"}
{"action":"lock","rat":"nr","type":3,"bands":"78,79"}
```

## 排障

```sh
# at-daemon 是否在、串口归谁
ubus list at-daemon -v
ubus call at-daemon list

# 服务日志（含 [UBUS] 自动识别结果）
logread | grep -E "mt5700m|UBUS"

# 手发 AT
ubus call at-daemon sendat '{"at_port":"/dev/ttyUSB1","at_cmd":"AT+CSQ"}'
```

| 现象 | 原因 / 处理 |
| --- | --- |
| 日志 `at-daemon open 失败` | QModem 没开 `use_ubus=1`，或 at-daemon 未运行 |
| 命令一直 `TIMEOUT` | `at_port` 与 at-daemon 实际打开的端口不一致；保持 `auto` |
| `settings_pending` | QModem 正在初始化并持有租约，几秒后自动恢复 |
| 面板能读但拨号按钮报错 | 预期行为：`managed=qmodem`，请到 QModem 操作 |
| 找不到模组 | XGPV3 转接板断电复位问题，拔 DC 等电容放电后再上电 |

## 编译（本地）

```bash
cargo test
cargo build --release
```

交叉编译由 OpenWrt buildroot 负责（`rust-package.mk` + `RUST_ARCH_DEPENDS`），
不需要在构建机上装 Rust。

## 与上游的关系

- 本仓库基于 [hexos/mt5700m-service](https://github.com/hexos/mt5700m-service) 的
  `f0539a3` 版本，新增：UBUS 连接模式接线、AT 端口自动识别、URC 事件总线、
  MHI/MBIM 网卡识别、`managed` 拨号归属、OpenWrt 打包与 LuCI 入口。
- 协议、前端页面与原版保持一致，原版前端可直接对接。

## License

Apache-2.0
