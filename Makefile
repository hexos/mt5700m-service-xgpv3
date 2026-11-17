#
# mt5700m-service —— MT5700M 模组管理面板（Rust）
#
# 与 at-webserver 一样走 OpenWrt 的 rust-package.mk：源码随仓库携带，
# 交叉编译时由 buildroot 提供 rust/host，不依赖宿主机的 cargo。
#
include $(TOPDIR)/rules.mk

PKG_NAME:=mt5700m-service
PKG_VERSION:=1.1.0
PKG_RELEASE:=1

PKG_LICENSE:=Apache-2.0
PKG_MAINTAINER:=hexos

PKG_BUILD_DEPENDS:=rust/host
PKG_BUILD_PARALLEL:=1
PKG_BUILD_DIR:=$(BUILD_DIR)/$(PKG_NAME)-$(PKG_VERSION)

include $(INCLUDE_DIR)/package.mk
include $(TOPDIR)/feeds/packages/lang/rust/rust-package.mk

define Package/mt5700m-service
  SECTION:=net
  CATEGORY:=Network
  TITLE:=MT5700M modem management service (Rust)
  URL:=https://github.com/hexos/mt5700m-service-xgpv3
  DEPENDS:=$(RUST_ARCH_DEPENDS) +ubus +uci
endef

define Package/mt5700m-service/description
  MT5700M 蜂窝模组管理服务：WebSocket + HTTP JSON API，内置独立 Web UI。
  在 XGPV3 上通过 QModem 的 at-daemon（ubus）共享 AT 串口，不与 QModem 抢锁。
endef

define Package/mt5700m-service/conffiles
/etc/config/mt5700m
endef

define Build/Prepare
	mkdir -p $(PKG_BUILD_DIR)
	$(CP) ./Cargo.toml ./Cargo.lock $(PKG_BUILD_DIR)/
	$(CP) ./src $(PKG_BUILD_DIR)/
endef

define Package/mt5700m-service/install
	$(INSTALL_DIR) $(1)/usr/bin
	$(INSTALL_BIN) $(PKG_INSTALL_DIR)/bin/mt5700m-service $(1)/usr/bin/mt5700m-service

	$(INSTALL_DIR) $(1)/etc/init.d
	$(INSTALL_BIN) ./files/etc/init.d/mt5700m-service $(1)/etc/init.d/mt5700m-service

	$(INSTALL_DIR) $(1)/etc/config
	$(INSTALL_CONF) ./files/etc/config/mt5700m $(1)/etc/config/mt5700m

	$(INSTALL_DIR) $(1)/etc/uci-defaults
	$(INSTALL_BIN) ./files/etc/uci-defaults/95-mt5700m $(1)/etc/uci-defaults/95-mt5700m

	# 服务自身托管的静态前端（http://<路由>:8088/）
	$(INSTALL_DIR) $(1)/usr/share/mt5700m/web
	$(CP) ./web/. $(1)/usr/share/mt5700m/web/

	# 同时安装到 /www/mt5700m/，由 uhttpd 提供 http://<路由>/mt5700m/，
	# 方便 LuCI 菜单内嵌（不开额外端口也能访问）。
	$(INSTALL_DIR) $(1)/www/mt5700m
	$(CP) ./web/. $(1)/www/mt5700m/
endef

$(eval $(call RustBinPackage,mt5700m-service))
$(eval $(call BuildPackage,mt5700m-service))
