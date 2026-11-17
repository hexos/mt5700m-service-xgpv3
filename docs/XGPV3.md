# XGPV3 适配说明（改动清单）

本文件记录相对上游 `f0539a3` 的**功能性**改动，便于 review 与后续跟进上游。

## 1. 为什么必须改

上游 `src/at/connection.rs` 已经写了 `UbusATConn` / `TomModemATConn`，但**没有接线**：

- `src/config.rs` 只接受 `auto|serial|network`，`mode=ubus` 会被归一化掉；
- `src/at/client.rs` 只按 `network` / 其它（串口）二选一，`UbusATConn` 永远用不到；
- `src/ubus.rs` 只有一个 `#[allow(dead_code)]` 的 `ubus_call`，无端口识别、无 URC 监听。

结果：在 XGPV3 上只能走串口独占模式，和 QModem 抢 `/dev/ttyUSBx`，
表现为两边的 AT 响应随机残缺/超时。

## 2. 改动清单

| 文件 | 改动 |
| --- | --- |
| `src/config.rs` | `mode` 支持 `ubus`/`tom_modem` 及别名；新增 `at_port_fallback`、`force_init`、`connection.managed`、`connection.netdev`；`at_port=auto` 判定 |
| `src/ubus.rs` | 重写：`ubus call -S -t` 封装、`at-daemon list` 端口挑选、`/dev` 节点扫描、URC 前缀注册、`ubus listen` 事件循环 + 去重 |
| `src/at/connection.rs` | `UbusATConn` 增加 `at_port/fallback/auto_detect`，连接时重新识别端口；`TomModemATConn` 使用 `tom_modem_feature`（`UBUS` → `-u`，修掉非法的 `-UBUS`） |
| `src/at/client.rs` | 按 `mode` 选择四种连接实现；UBUS 模式下跳过 `init_module`（除非 `force_init=1`） |
| `src/main.rs` | `auto` 优先探测 at-daemon；`ubus` 启动时自动识别端口；启动 URC 事件监听任务 |
| `src/usb.rs` | `netdev()` 增加 MHI/MBIM/QMI 驱动兜底识别（PCIe 转接板场景无 `3466:3301` USB 节点）；新增 `module_present()` |
| `src/connection_mgr.rs` | `managed=qmodem` 时跳过 `network` UCI 同步与 `AT^NDISDUP`，连接类 action 返回明确提示；状态 JSON 增加 `managed`/`at_mode`，无 USB 节点时报 `pcie` |
| `src/api.rs` | `config` action 暴露新增字段 |
| `web/js/pages/settings.js` | 连接方式增加 `QModem at-daemon` / `tom_modem`，增加兜底串口输入 |
| `Makefile` | 改为标准 OpenWrt Rust 包（`rust-package.mk` + `RustBinPackage`），安装 init.d/config/uci-defaults/静态页 |
| `files/etc/config/mt5700m` | XGPV3 默认：`mode=ubus`、`at_port=auto`、`managed=qmodem` |
| `files/etc/uci-defaults/95-mt5700m` | 首次启动写入默认值；仅翻转 QModem 已存在的 `use_ubus` 选项 |
| `luci-app-mt5700m/` | LuCI 菜单 + ACL + 内嵌 `/mt5700m/` 的 view |

## 3. 关键取舍

1. **不抢租约**：只调 `sendat`，不调 `lease_acquire`；`settings_pending` 时最多重试 8×250ms。
2. **不改 QModem 的 URC 配置**：UBUS 模式不重发 `AT+CNMI`，避免 SMS 通知被改坏。
3. **拨号默认归 QModem**：避免两套管理器争同一个网卡；独立 USB 场景可切 `managed=self`。
4. **失败可回退**：把 `mode` 改回 `serial`（或 `network`）重启服务即可，无需重刷。

## 4. 验证清单（刷机后）

```sh
# 1. 服务在跑
pgrep -f mt5700m-service

# 2. 自动识别结果
logread | grep "\[UBUS\] 自动识别"

# 3. 共享通道可用
ubus call at-daemon list
ubus call at-daemon sendat '{"at_port":"/dev/ttyUSB1","at_cmd":"AT+CSQ"}'

# 4. HTTP API
curl -s http://127.0.0.1:8088/api/status | head -c 400

# 5. 面板
#    http://<路由>/mt5700m/ 与 LuCI -> 网络 -> MT5700M 模组
```

## 5. 上游跟进

改动集中在 6 个 Rust 文件 + 打包层，未触碰 `parse.rs` / `queries.rs` / `actions.rs` /
`traffic.rs` / `sms.rs` 的业务逻辑，便于后续 rebase 上游。
