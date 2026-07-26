################################################################################
#
# rulesyos-stage0
#
################################################################################

RULESYOS_STAGE0_VERSION = 0.1.0
RULESYOS_STAGE0_SITE = $(BR2_EXTERNAL_RULESYOS_PATH)/..
RULESYOS_STAGE0_SITE_METHOD = local
RULESYOS_STAGE0_SUBDIR = crates/rulesyos-stage0
RULESYOS_STAGE0_LICENSE = MIT
RULESYOS_STAGE0_OVERRIDE_SRCDIR_RSYNC_EXCLUSIONS = \
	--exclude=/.cache \
	--exclude=/output \
	--exclude=/br2-external \
	--exclude=/target

define RULESYOS_STAGE0_VENDOR_DEPENDENCIES
	mkdir -p $(@D)/.cargo $(BR_CARGO_HOME)
	cd $(@D) && \
		$(HOST_MAKE_ENV) \
		CARGO_HOME=$(BR_CARGO_HOME) \
		flock $(BR_CARGO_HOME)/.br-lock \
		cargo vendor \
			--manifest-path Cargo.toml \
			--locked \
			VENDOR \
			> .cargo/config
endef
RULESYOS_STAGE0_PRE_CONFIGURE_HOOKS += RULESYOS_STAGE0_VENDOR_DEPENDENCIES

$(eval $(cargo-package))
