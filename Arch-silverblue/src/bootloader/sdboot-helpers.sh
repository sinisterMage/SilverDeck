# shellcheck shell=bash
#
# sdboot-helpers.sh — systemd-boot integration for Arch Silverblue.
#
# Sourced by src/update-engine/silverblue-update and src/init/silverblue-mark-good.sh.
# Pure render/predicate functions are self-contained and safe to source for unit tests.
# IO functions rely on log()/sb_run()/emit_file() provided by the sourcing engine.
#
# systemd-boot can only read the FAT ESP, so each snapshot's kernel + initramfs are
# COPIED out of the (Btrfs) subvolume into $EFI/silverblue/<snap>/ and referenced from a
# Boot Loader Specification Type #1 entry. Auto-rollback uses native boot counting: the
# entry filename carries a "+N" tries suffix; systemd-boot decrements it every boot and
# demotes the entry at +0, so the previous (counter-less) entry boots instead.

# Strip the snapshot prefix to obtain a sortable version string (newest sorts first).
sdboot_version_of() {
    local snap=$1
    printf '%s\n' "${snap#root-}"
}

# Return 0 if systemd-boot appears to be the active bootloader under $1 (the ESP mount).
sdboot_is_active() {
    local efi=$1
    [[ -e "$efi/EFI/systemd/systemd-bootx64.efi" ]] && return 0
    [[ -e "$efi/EFI/BOOT/BOOTX64.EFI" && -f "$efi/loader/loader.conf" ]] && return 0
    return 1
}

# Render a Type #1 loader entry to stdout. Pure: no IO.
#   $1 snap   $2 label   $3 pool_uuid   $4 kernel_basename   $5.. initrd_basenames
sdboot_render_entry() {
    local snap=$1 label=$2 uuid=$3 kernel=$4
    shift 4
    local version initrd
    version=$(sdboot_version_of "$snap")
    printf 'title    %s\n' "$label"
    printf 'sort-key %s\n' "${SB_SORT_KEY:-silverblue}"
    printf 'version  %s\n' "$version"
    printf 'linux    /%s/%s/%s\n' "${SB_ESP_SUBDIR:-silverblue}" "$snap" "$kernel"
    for initrd in "$@"; do
        printf 'initrd   /%s/%s/%s\n' "${SB_ESP_SUBDIR:-silverblue}" "$snap" "$initrd"
    done
    printf 'options  root=UUID=%s rootflags=subvol=%s rootfstype=btrfs rw%s\n' \
        "$uuid" "$snap" "${SB_KERNEL_OPTS:+ $SB_KERNEL_OPTS}"
}

# Order initramfs/microcode images: microcode first, primary initramfs last, skip fallback.
# Echoes the chosen initrd basenames (one per line) found in directory $1.
sdboot_initrd_list() {
    local boot=$1 img base
    for img in "$boot"/*-ucode.img; do
        [[ -e "$img" ]] || continue
        printf '%s\n' "${img##*/}"
    done
    for img in "$boot"/initramfs-*.img; do
        [[ -e "$img" ]] || continue
        base=${img##*/}
        [[ "$base" == *-fallback.img ]] && continue
        printf '%s\n' "$base"
    done
}

# Copy a snapshot's kernel + initramfs + microcode from its /boot into the ESP and write
# the loader entry with boot counting enabled.
#   $1 snap  $2 label  $3 pool_uuid  $4 snapshot_boot_dir  $5 efi_dir  $6 tries
sdboot_register_entry() {
    local snap=$1 label=$2 uuid=$3 src_boot=$4 efi=$5 tries=$6
    local dst="$efi/${SB_ESP_SUBDIR:-silverblue}/$snap"
    local entries="$efi/loader/entries"
    local img kernel="" initrds=()

    sb_run mkdir -p "$dst" "$entries"

    for img in "$src_boot"/vmlinuz-*; do
        [[ -e "$img" ]] || continue
        sb_run cp -f -- "$img" "$dst/"
        [[ -n "$kernel" ]] || kernel=${img##*/}
    done
    for img in "$src_boot"/initramfs-*.img "$src_boot"/*-ucode.img; do
        [[ -e "$img" ]] || continue
        sb_run cp -f -- "$img" "$dst/"
    done

    if [[ -z "$kernel" ]]; then
        log "error: no kernel found in $src_boot"
        return 1
    fi

    mapfile -t initrds < <(sdboot_initrd_list "$src_boot")

    sdboot_render_entry "$snap" "$label" "$uuid" "$kernel" "${initrds[@]}" \
        | emit_file "$entries/${snap}+${tries}.conf"
}

# Set the next boot to a specific snapshot without changing the permanent default.
# Used by --rollback to force an OLDER entry; a freshly registered entry is already the
# next boot by virtue of having the newest version, so normal updates need no oneshot.
sdboot_set_next() {
    local snap=$1
    sb_run "$BOOTCTL" set-oneshot "${snap}.conf"
}

# Locate the on-disk loader entry for a snapshot (with or without a tries suffix).
sdboot_entry_path() {
    local snap=$1 efi=$2 f
    for f in "$efi/loader/entries/${snap}.conf" "$efi/loader/entries/${snap}+"*.conf; do
        [[ -e "$f" ]] && { printf '%s\n' "$f"; return 0; }
    done
    return 1
}

# Mark the current snapshot good: drop the boot-counting suffix so it is no longer retried
# and, being the newest version, becomes the de-facto permanent default.
#   $1 snap  $2 efi_dir
sdboot_mark_good() {
    local snap=$1 efi=$2 cur=""
    if command -v systemd-bless-boot >/dev/null 2>&1 \
        && [[ -e /sys/firmware/efi/efivars ]]; then
        sb_run systemd-bless-boot good && return 0
    fi
    cur=$(sdboot_entry_path "$snap" "$efi") || {
        log "warn: no loader entry for $snap to bless"
        return 0
    }
    [[ "$cur" == "$efi/loader/entries/${snap}.conf" ]] && return 0
    sb_run mv -f -- "$cur" "$efi/loader/entries/${snap}.conf"
}

# Remove a snapshot's loader entry and its copied kernels from the ESP.
#   $1 snap  $2 efi_dir
sdboot_prune_entry() {
    local snap=$1 efi=$2 f
    for f in "$efi/loader/entries/${snap}.conf" "$efi/loader/entries/${snap}+"*.conf; do
        [[ -e "$f" ]] && sb_run rm -f -- "$f"
    done
    [[ -d "$efi/${SB_ESP_SUBDIR:-silverblue}/$snap" ]] && sb_run rm -rf -- "$efi/${SB_ESP_SUBDIR:-silverblue}/$snap"
    return 0
}
