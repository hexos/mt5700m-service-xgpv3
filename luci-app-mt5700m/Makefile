include $(TOPDIR)/rules.mk

PKG_NAME:=luci-app-mt5700m
PKG_VERSION:=1.1.0
PKG_RELEASE:=1
PKG_LICENSE:=Apache-2.0
PKG_MAINTAINER:=hexos

include $(INCLUDE_DIR)/package.mk

define Package/luci-app-mt5700m
  SECTION:=luci
  CATEGORY:=LuCI
  SUBMENU:=3. Applications
  TITLE:=LuCI entry for MT5700M modem panel
  DEPENDS:=+luci-base +mt5700m-service
  PKGARCH:=all
endef

define Package/luci-app-mt5700m/description
  LuCI menu entry and ACL for the MT5700M modem panel served at /mt5700m/.
endef

# 纯文件包没有源码目录，必须显式给空的 Compile，否则 package.mk 的默认
# Build/Compile/Default 会去 build_dir/<name>-<ver> 里跑 make，
# 报 "No targets specified and no makefile found"。
define Build/Compile
endef

define Package/luci-app-mt5700m/install
	$(INSTALL_DIR) $(1)/usr/share/luci/menu.d
	$(INSTALL_DATA) ./root/usr/share/luci/menu.d/luci-app-mt5700m.json $(1)/usr/share/luci/menu.d/
	$(INSTALL_DIR) $(1)/usr/share/rpcd/acl.d
	$(INSTALL_DATA) ./root/usr/share/rpcd/acl.d/luci-app-mt5700m.json $(1)/usr/share/rpcd/acl.d/
	$(INSTALL_DIR) $(1)/www/luci-static/resources/view/mt5700m
	$(INSTALL_DATA) ./htdocs/luci-static/resources/view/mt5700m/overview.js $(1)/www/luci-static/resources/view/mt5700m/
endef

$(eval $(call BuildPackage,luci-app-mt5700m))
