# Arch Silverblue — top-level build/test orchestration.
#
# The dev host only needs docker + qemu. shellcheck and bats are pulled on demand via
# `nix shell`, so no Arch tooling is required on the host (all Arch tooling runs inside the
# build container and the QEMU guest).

SHELL := bash
# Note: '#' must be escaped in Makefiles or it starts a comment.
NIXRUN := nix shell nixpkgs\#shellcheck nixpkgs\#bats --command

# Every shell script that must pass shellcheck.
SHELL_FILES := \
	src/update-engine/silverblue-update \
	src/bootloader/sdboot-helpers.sh \
	src/bootloader/grub-helpers.sh \
	src/init/silverblue-mark-good.sh \
	src/init/silverblue-rollback.sh \
	src/installer/install-lib.sh \
	src/installer/silverblue-install \
	iso/build.sh \
	iso/airootfs/usr/local/bin/silverblue-autoinstall.sh \
	tests/qemu/run.sh \
	tests/unit/run.sh \
	tools/verify-units.sh \
	tests/unit/mocks/btrfs \
	tests/unit/mocks/bootctl \
	tests/unit/mocks/grub-editenv \
	tests/unit/mocks/arch-chroot

# Docker image tag derived from the distro id in config/distro.conf (bash, not sh: the config
# uses arrays). Falls back to the upstream name if the config is missing.
IMAGE := $(shell bash -c '. config/distro.conf 2>/dev/null && printf "%s-iso" "$$DISTRO_ID"' 2>/dev/null || echo arch-silverblue-iso)
ISO_STAMP := iso/output/.built
ISO_INPUTS := iso/Dockerfile iso/build.sh $(shell find iso/airootfs src config iso/local-repo -type f 2>/dev/null)

.PHONY: all lint test-unit verify-units build-iso rebuild-iso test-qemu test-qemu-interactive test ci clean help

help:
	@echo "Targets:"
	@echo "  lint          shellcheck every shell script (zero findings)"
	@echo "  test-unit     run the bats unit tests (no root/Btrfs needed)"
	@echo "  verify-units  systemd-analyze verify the Silverblue systemd units"
	@echo "  build-iso     build the bootable ISO via Docker (needs --privileged + network)"
	@echo "  test-qemu     boot the ISO in QEMU and run the happy-path + rollback tests"
	@echo "  test-qemu-interactive  drive the interactive installer in QEMU and boot the result"
	@echo "  test          lint + test-unit + verify-units (fast inner loop)"
	@echo "  ci            test + build-iso + test-qemu (full pipeline)"
	@echo "  clean         remove build/test artifacts"

# --- Fast inner loop (host-only, no docker/qemu) ------------------------------------------
# --source-path=SCRIPTDIR lets each script's `# shellcheck source=` directive resolve relative
# to its own directory (build.sh / the autoinstaller follow config/distro.conf this way).
lint:
	$(NIXRUN) shellcheck -x --source-path=SCRIPTDIR $(SHELL_FILES)

test-unit:
	$(NIXRUN) bats tests/unit

verify-units:
	bash tools/verify-units.sh

test: lint test-unit verify-units

# --- ISO build (Arch tooling inside a privileged container) -------------------------------
build-iso: $(ISO_STAMP)

$(ISO_STAMP): $(ISO_INPUTS)
	docker build -t $(IMAGE) -f iso/Dockerfile .
	@mkdir -p iso/output
	docker run --rm --privileged -v "$(CURDIR):/build" $(IMAGE)
	@touch $(ISO_STAMP)
	@ls -lh iso/output/*.iso

rebuild-iso:
	@rm -f $(ISO_STAMP)
	@$(MAKE) build-iso

# --- QEMU integration test ----------------------------------------------------------------
test-qemu: $(ISO_STAMP)
	bash tests/qemu/run.sh

test-qemu-interactive: $(ISO_STAMP)
	bash tests/qemu/run.sh --interactive

ci: test build-iso test-qemu

clean:
	rm -rf iso/output tests/qemu/work
