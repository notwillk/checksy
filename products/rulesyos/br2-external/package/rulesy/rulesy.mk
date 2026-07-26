################################################################################
#
# rulesy
#
################################################################################

$(eval $(file <$(BR2_EXTERNAL_RULESYOS_PATH)/../rulesy-release.lock))

RULESY_VERSION = $(RULESY_RELEASE_VERSION)
RULESY_SOURCE = $(RULESY_RELEASE_ARCHIVE)
RULESY_SITE = $(patsubst %/,%,$(dir $(RULESY_RELEASE_URL)))
RULESY_STRIP_COMPONENTS = 0
RULESY_LICENSE = MIT

define RULESY_VERIFY_ARCHIVE
	printf '%s  %s\n' \
		'$(RULESY_RELEASE_ARCHIVE_SHA256)' \
		'$(RULESY_DL_DIR)/$(RULESY_SOURCE)' | sha256sum --check -
endef
RULESY_POST_DOWNLOAD_HOOKS += RULESY_VERIFY_ARCHIVE

define RULESY_VERIFY_BINARY
	printf '%s  %s\n' \
		'$(RULESY_RELEASE_BINARY_SHA256)' '$(@D)/rulesy' | \
		sha256sum --check -
endef
RULESY_POST_EXTRACT_HOOKS += RULESY_VERIFY_BINARY

define RULESY_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/rulesy $(TARGET_DIR)/usr/bin/rulesy
endef

$(eval $(generic-package))
