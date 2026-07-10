# Building & developing

Host expectations: **Nix** (dev shells — no system Rust/Arch tooling needed)
and **Docker** (all Arch-side work runs in containers). QEMU + OVMF for the
integration tests.

## UI inner loop (no image involved)

```sh
make ui-check            # rustfmt + clippy -D warnings + tests (whole workspace)
make ui-run              # console shell in your Wayland session (fake games)
make ui-run-kiosk        # nested sway kiosk (validates fullscreen/IPC rules)
make ui-run-lavapipe     # software-Vulkan path (what QEMU uses)

make installer-run       # GUI installer against the fake engine (no root, no disks)
make installer-run-kiosk # same, in a nested sway kiosk
```

The installer's fake engine (`SILVERDECK_FAKE_INSTALL=1`, set by those
targets) replays a realistic marker stream. Useful variants while iterating:

```sh
SILVERDECK_FAKE_INSTALL=fail  cargo run -p silverdeck-installer  # exercise the failure screen
SILVERDECK_FAKE_OFFLINE=1 SILVERDECK_FAKE_INSTALL=1 \
                              cargo run -p silverdeck-installer  # exercise the Wi-Fi screen
```

## Image pipeline

```sh
make ui-package   # build the 4 pacman packages in the Arch builder container
                  #   -> Arch-silverblue/iso/local-repo/
make build-iso    # ui-package + archiso build (privileged Docker)
                  #   -> Arch-silverblue/iso/output/silverdeck-*.iso + SHA256SUMS
make test-qemu    # unattended install + update + rollback asserts in QEMU
make test         # ui-check + the toolkit's shellcheck/bats/verify-units
```

`make -C Arch-silverblue test-qemu-interactive` drives the *text* installer
over the serial console and boots the result — this is the regression gate for
the installer prompt contract.

## Watching the GUI installer in QEMU

The harness runs headless; to see the live kiosk with your own eyes:

```sh
make build-iso
cd Arch-silverblue
qemu-img create -f qcow2 /tmp/sd-test.qcow2 20G
qemu-system-x86_64 \
  -machine q35 -enable-kvm -cpu host -m 4096 -smp 4 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd \
  -device virtio-gpu-pci -display gtk \
  -device qemu-xhci -device usb-kbd \
  -drive file=/tmp/sd-test.qcow2,if=virtio,format=qcow2 \
  -netdev user,id=net0 -device virtio-net-pci,netdev=net0 \
  -cdrom iso/output/silverdeck-*.iso
```

(OVMF path varies by distro; on NixOS run
`nix build nixpkgs#OVMF.fd --out-link /tmp/ovmf` — note the link is created as
`/tmp/ovmf-fd` because of the `.fd` output — and use
`/tmp/ovmf-fd/FV/OVMF_CODE.fd`, plus a writable copy of
`/tmp/ovmf-fd/FV/OVMF_VARS.fd` as the second pflash drive.) The VM boots the
GUI installer on the virtio-gpu display using software rendering (sway on
pixman, the installer on lavapipe) — the same path the session script picks on
any machine without a usable render node.

## Regenerating the logo assets

The logo source is `silverdeck-ui/assets/logo.svg`; the rendered PNGs are
committed so builds don't need an SVG renderer:

```sh
nix shell nixpkgs#librsvg --command \
  rsvg-convert -w 192 -h 192 silverdeck-ui/assets/logo.svg \
    -o packaging/silverdeck-plymouth/files/logo.png
```

## CI

`.github/workflows/ci.yml` mirrors the make targets: `ui` and `shell` jobs on
every push/PR; the `iso` job on default-branch pushes and manual dispatch
(with a disk-space free step — mkarchiso needs ~25 GB); the `qemu` job on
manual dispatch only (no KVM on hosted runners → TCG is slow).
`.github/workflows/release.yml` builds the ISO on `v*` tags and publishes a
GitHub Release with the ISO, checksums, and the pacman packages.
