include $(TOPDIR)/rules.mk

PKG_NAME:=mt5700m-service
PKG_VERSION:=1.0.0
PKG_RELEASE:=1

PKG_SOURCE_PROTO:=git
PKG_SOURCE_URL:=https://github.com/YOUR_ORG/mt5700m-service.git
PKG_SOURCE_VERSION:=main
PKG_SOURCE_SUBMODULES:=

PKG_LICENSE:=Apache-2.0

include $(INCLUDE_DIR)/package.mk

define Package/mt5700m-service
	SECTION:=net
	CATEGORY:=Network
	TITLE:=MT5700M modem management service (Rust)
	DEPENDS:=+libstdcpp +libopenssl
endef

define Package/mt5700m-service/description
	Rust implementation of the luci-app-mt5700m backend. Provides a WebSocket
	(and minimal HTTP) JSON API for MT5700M status, network, system, SMS,
	frequency lock and connection management. Protocol is a superset of the
	at-webserver WebSocket protocol in mt5700webui-openwrt-server.
endef

define Build/Compile
	$(call Build/Compile/Default)
	( cd $(PKG_BUILD_DIR) && cargo build --release )
endef

define Package/mt5700m-service/install
	$(INSTALL_DIR) $(1)/usr/bin
	$(INSTALL_BIN) $(PKG_BUILD_DIR)/target/*/release/mt5700m-service $(1)/usr/bin/

	$(INSTALL_DIR) $(1)/etc/init.d
	$(INSTALL_BIN) ./files/etc/init.d/mt5700m-service $(1)/etc/init.d/

	$(INSTALL_DIR) $(1)/etc/config
	$(INSTALL_DATA) ./files/etc/config/mt5700m $(1)/etc/config/
endef

$(eval $(call BuildPackage,mt5700m-service))
