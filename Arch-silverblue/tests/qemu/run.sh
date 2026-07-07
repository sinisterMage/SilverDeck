#!/usr/bin/env bash
#
# run.sh — QEMU integration test for Arch Silverblue (systemd-boot path).
#
# Boots the built ISO headless, installs Arch Silverblue to a virtual disk, then drives the
# installed system over the serial console to exercise:
#   * happy path : first boot marks the root good; one update cycle; reboot; assert good
#   * rollback   : a "bad update" (corrupt new-root kernel) reverts to the previous root
#
# This shell wrapper prepares the environment (ISO, disk, firmware, acceleration) and hands
# off to harness.py, which does the serial-console expect/assert work. KVM is used when
# available, otherwise TCG (-cpu qemu64); no KVM is required.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
WORK="$ROOT_DIR/tests/qemu/work"

# Derivative identity: the harness drives ${BIN_PREFIX}-update / ${UNIT_PREFIX}-* on the
# guest, and asserts the kiosk session when the derivative enables one (greetd in
# ENABLE_SERVICES). Stock config reproduces the upstream silverblue names.
# shellcheck source=../../config/distro.conf
source "$ROOT_DIR/config/distro.conf"
SESSION=0
case " ${ENABLE_SERVICES[*]:-} " in *" greetd.service "*) SESSION=1 ;; esac

NET=0
BOOTLOADER=systemd-boot
INTERACTIVE=0
DISK_SIZE=${SB_DISK_SIZE:-12G}

usage() {
    cat <<'EOF'
Usage: tests/qemu/run.sh [--net] [--bootloader systemd-boot|grub] [--interactive]

  --net          Update cycle does a real `pacman -Syu` over QEMU user-net (default: a
                 hermetic offline upgrade against the ISO's synthetic local repo).
  --bootloader   Bootloader to install on the target and drive through the full
                 install/update/rollback cycle: systemd-boot | grub (default: systemd-boot).
  --interactive  Drive the interactive installer over the serial console instead of the
                 unattended autoinstaller, then boot and verify the installed system.
  -h, --help     Show this help.

Environment overrides: SB_OVMF_CODE, SB_OVMF_VARS (firmware), SB_DISK_SIZE.
EOF
}

while (( $# )); do
    case "$1" in
        --net)         NET=1 ;;
        --bootloader)  BOOTLOADER=${2:?}; shift ;;
        --interactive) INTERACTIVE=1 ;;
        -h|--help)     usage; exit 0 ;;
        *)             echo "unknown argument: $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done

command -v qemu-system-x86_64 >/dev/null 2>&1 || { echo "qemu-system-x86_64 not found" >&2; exit 1; }
command -v qemu-img >/dev/null 2>&1 || { echo "qemu-img not found" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "python3 not found" >&2; exit 1; }

shopt -s nullglob
isos=("$ROOT_DIR"/iso/output/*.iso)
shopt -u nullglob
(( ${#isos[@]} > 0 )) || { echo "no ISO in iso/output/ — run 'make build-iso' first" >&2; exit 1; }
ISO=${isos[0]}
for f in "${isos[@]}"; do [[ "$f" -nt "$ISO" ]] && ISO=$f; done
echo "ISO: $ISO"

# --- Acceleration: KVM if usable, else TCG ------------------------------------------------
if [[ -w /dev/kvm ]]; then
    ACCEL=kvm; CPU=host
    echo "acceleration: KVM (-cpu host)"
else
    ACCEL=tcg; CPU=qemu64
    echo "acceleration: TCG (-cpu qemu64) — no KVM; this is slow but supported"
fi

# --- Locate OVMF (UEFI firmware) ----------------------------------------------------------
FW_CODE=""
FW_VARS_SRC=""
resolve_ovmf() {
    if [[ -n "${SB_OVMF_CODE:-}" && -n "${SB_OVMF_VARS:-}" ]]; then
        FW_CODE=$SB_OVMF_CODE; FW_VARS_SRC=$SB_OVMF_VARS; return 0
    fi
    local pair c v
    for pair in \
        "/usr/share/edk2/x64/OVMF_CODE.4m.fd:/usr/share/edk2/x64/OVMF_VARS.4m.fd" \
        "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd:/usr/share/edk2-ovmf/x64/OVMF_VARS.fd" \
        "/usr/share/OVMF/OVMF_CODE.fd:/usr/share/OVMF/OVMF_VARS.fd" \
        "/usr/share/qemu/edk2-x86_64-code.fd:/usr/share/qemu/edk2-i386-vars.fd"; do
        c=${pair%%:*}; v=${pair#*:}
        if [[ -r "$c" && -r "$v" ]]; then FW_CODE=$c; FW_VARS_SRC=$v; return 0; fi
    done
    if command -v nix >/dev/null 2>&1; then
        local p
        p=$(nix build --no-link --print-out-paths 'nixpkgs#OVMF.fd' 2>/dev/null | head -1 || true)
        if [[ -n "$p" && -r "$p/FV/OVMF_CODE.fd" && -r "$p/FV/OVMF_VARS.fd" ]]; then
            FW_CODE="$p/FV/OVMF_CODE.fd"; FW_VARS_SRC="$p/FV/OVMF_VARS.fd"; return 0
        fi
    fi
    echo "could not locate OVMF firmware; set SB_OVMF_CODE and SB_OVMF_VARS" >&2
    return 1
}
resolve_ovmf
echo "firmware: $FW_CODE"

# --- Fresh disk + writable firmware vars (persist EFI entries across phases) ---------------
mkdir -p "$WORK"
DISK="$WORK/silverblue-test.qcow2"
rm -f "$DISK" "$WORK/OVMF_VARS.fd"
qemu-img create -f qcow2 "$DISK" "$DISK_SIZE" >/dev/null
cp -f "$FW_VARS_SRC" "$WORK/OVMF_VARS.fd"
chmod u+w "$WORK/OVMF_VARS.fd"

export SB_ISO="$ISO" SB_DISK="$DISK"
export SB_FW_CODE="$FW_CODE" SB_FW_VARS="$WORK/OVMF_VARS.fd"
export SB_ACCEL="$ACCEL" SB_CPU="$CPU" SB_NET="$NET" SB_BOOTLOADER="$BOOTLOADER"
export SB_INTERACTIVE="$INTERACTIVE"
export SB_WORK="$WORK"
export SB_BIN_PREFIX="$BIN_PREFIX" SB_UNIT_PREFIX="$UNIT_PREFIX" SB_ESP_SUBDIR="$ESP_SUBDIR"
export SB_SESSION="$SESSION"

echo "==> launching harness (net=$NET bootloader=$BOOTLOADER interactive=$INTERACTIVE)"
exec python3 "$ROOT_DIR/tests/qemu/harness.py"
