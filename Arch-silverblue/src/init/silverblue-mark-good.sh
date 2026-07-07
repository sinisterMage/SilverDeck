#!/usr/bin/env bash
#
# silverblue-mark-good.sh — run once after boot by silverblue-mark-good.service.
#
# If the boot is healthy it marks the running snapshot "good": drops the systemd-boot
# boot-counting suffix (or updates grubenv's saved_entry) so the snapshot becomes the
# permanent default, then prunes old snapshots. If the boot is unhealthy it exits non-zero
# so the unit's OnFailure handler (silverblue-rollback.target) reverts to the previous root.
#
# It reuses the update engine's logic by sourcing it (the engine only defines functions when
# sourced), so bootloader detection / marking / pruning live in exactly one place.
#
# Like the engine, this file is sourceable: it only runs mark_good_main (and enables strict mode)
# when executed directly, so the unit tests can source it and call health_check in isolation.

find_engine() {
    local self d
    self=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
    for d in \
        "$self/../update-engine/silverblue-update" \
        "$self/silverblue-update" \
        /usr/bin/silverblue-update \
        /usr/lib/silverblue/silverblue-update; do
        [[ -f "$d" ]] && { printf '%s\n' "$d"; return 0; }
    done
    return 1
}

_engine=$(find_engine) || { printf 'error: silverblue-update engine not found\n' >&2; exit 1; }
# shellcheck source=/dev/null
source "$_engine"

# Markers go to stdout so they land in the journal (and over the serial console in tests).
marker() { printf '%s\n' "$*"; }

# Returns 0 if the current boot is healthy. Override the check with SB_HEALTHCHECK_CMD
# (used by the QEMU rollback test to force a failure deterministically).
health_check() {
    if [[ -n "${SB_HEALTHCHECK_CMD:-}" ]]; then
        bash -c "$SB_HEALTHCHECK_CMD"
        return $?
    fi
    local state u
    state=$(systemctl is-system-running 2>/dev/null || true)
    vlog "system state: ${state:-unknown}"
    case "$state" in
        running|starting|initializing)
            return 0
            ;;
        degraded)
            for u in local-fs.target sysinit.target; do
                if [[ "$(systemctl is-active "$u" 2>/dev/null || true)" != active ]]; then
                    err "critical unit not active: $u"
                    return 1
                fi
            done
            return 0
            ;;
        *)
            err "unexpected system state: ${state:-unknown}"
            return 1
            ;;
    esac
}

mark_good_main() {
    # No-op on the live install medium: it is "enabled" there but must not act (the ISO has
    # no Silverblue boot entries, and acting would needlessly trigger a rollback/reboot).
    if [[ -e /run/archiso ]] || grep -qw archisobasedir /proc/cmdline 2>/dev/null; then
        marker "SILVERBLUE-MARKGOOD-SKIP (live ISO environment)"
        exit 0
    fi

    CURRENT_SUBVOL=$(get_current_subvol)
    # Consumed by the sourced bootloader helpers when regenerating entries during prune.
    # shellcheck disable=SC2034
    SB_KERNEL_OPTS=$(kernel_extra_opts)
    BOOTLOADER=$(detect_bootloader "$SB_EFI_DIR") || { err "no bootloader detected"; exit 1; }
    log "mark-good: current=${CURRENT_SUBVOL:-unknown} bootloader=$BOOTLOADER"

    if ! health_check; then
        marker "SILVERBLUE-MARKGOOD-FAIL current=${CURRENT_SUBVOL:-unknown}"
        err "health check failed; rollback will be triggered"
        exit 1
    fi

    case "$BOOTLOADER" in
        systemd-boot) sdboot_mark_good "$CURRENT_SUBVOL" "$SB_EFI_DIR" ;;
        grub)         grub_mark_good "$CURRENT_SUBVOL" "$SB_EFI_DIR" ;;
        *)            err "unsupported bootloader: $BOOTLOADER"; exit 1 ;;
    esac

    detect_root_storage
    ensure_toplevel_mounted
    prune_snapshots
    cleanup_toplevel

    marker "SILVERBLUE-MARKGOOD-OK current=${CURRENT_SUBVOL:-unknown}"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    set -uo pipefail
    mark_good_main "$@"
fi
