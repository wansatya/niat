MCP_CLIENT_VERSION = 0.1.0
MCP_CLIENT_SITE = $(BR2_EXTERNAL_NIAT_PATH)/../../crates/mcp-client
MCP_CLIENT_SITE_METHOD = local
MCP_CLIENT_DEPENDENCIES = host-rustc

$(eval $(cargo-package))
