AGENT_KERNEL_VERSION = 0.1.0
AGENT_KERNEL_SITE = $(BR2_EXTERNAL_NIAT_PATH)/../../crates/agent-kernel
AGENT_KERNEL_SITE_METHOD = local
AGENT_KERNEL_DEPENDENCIES = host-rustc

$(eval $(cargo-package))
