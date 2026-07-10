# SilverDeck top-level orchestration.
#
# UI inner loop (host, no ISO):   make ui-check ui-run
# Full image:                     make build-iso   (docker)
# End-to-end:                     make test-qemu   (qemu)

UI_DIR      := silverdeck-ui
ASB_DIR     := Arch-silverblue
REPO_OUT    := $(ASB_DIR)/iso/local-repo
NIXDEV      := nix develop path:$(CURDIR)/$(UI_DIR) --command

.PHONY: ui-check ui-run ui-run-kiosk ui-run-lavapipe installer-run installer-run-kiosk ui-package build-iso test-qemu test clean

ui-check:
	cd $(UI_DIR) && $(NIXDEV) cargo fmt --all --check
	cd $(UI_DIR) && $(NIXDEV) cargo clippy --workspace --all-targets -- -D warnings
	cd $(UI_DIR) && $(NIXDEV) cargo test --workspace

ui-run:
	cd $(UI_DIR) && $(NIXDEV) cargo run -p silverdeck-app

# Validate kiosk behavior (fullscreen rules, sway IPC focus) in a nested sway.
ui-run-kiosk:
	cd $(UI_DIR) && $(NIXDEV) sh -c 'cargo build -p silverdeck-app && sway -c dev/sway.config'

# GUI installer against the fake engine (full flow, no root, no disks touched).
installer-run:
	cd $(UI_DIR) && $(NIXDEV) env SILVERDECK_FAKE_INSTALL=1 cargo run -p silverdeck-installer

installer-run-kiosk:
	cd $(UI_DIR) && $(NIXDEV) sh -c 'cargo build -p silverdeck-installer && sway -c dev/installer-sway.config'

# Prove the software-Vulkan path used inside QEMU (lavapipe).
ui-run-lavapipe:
	cd $(UI_DIR) && $(NIXDEV) sh -c 'VK_ICD_FILENAMES=$$LAVAPIPE_ICD cargo run -p silverdeck-app'

# Build silverdeck-ui + silverdeck-session pacman packages in an Arch
# container and publish them as a local repo consumed by the ISO build.
ui-package:
	bash packaging/build-repo.sh

build-iso: ui-package
	$(MAKE) -C $(ASB_DIR) build-iso

test-qemu:
	$(MAKE) -C $(ASB_DIR) test-qemu

# Upstream distro checks (shellcheck/bats must stay green on our fork) + UI checks.
test: ui-check
	$(MAKE) -C $(ASB_DIR) test

clean:
	rm -rf $(REPO_OUT)
	cd $(UI_DIR) && cargo clean 2>/dev/null || true
