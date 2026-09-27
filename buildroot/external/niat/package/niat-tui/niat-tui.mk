NIAT_TUI_VERSION = 0.1.0
NIAT_TUI_SITE = $(BR2_EXTERNAL_NIAT_PATH)/../../crates/niat-tui
NIAT_TUI_SITE_METHOD = local
NIAT_TUI_DEPENDENCIES = host-rustc

$(eval $(cargo-package))
