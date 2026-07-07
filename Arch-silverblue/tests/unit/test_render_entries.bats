#!/usr/bin/env bats
# Bootloader entry rendering and registration (with mocked external commands).

load helper

setup() {
    load_engine
    TMP="$(mktemp -d)"
}

teardown() { rm -rf "$TMP"; }

@test "sdboot_render_entry produces a valid Type #1 entry" {
    run sdboot_render_entry "root-20260628-093000" "Arch Silverblue 2026-06-28 09:30" \
        "POOLUUID" "vmlinuz-linux" "intel-ucode.img" "initramfs-linux.img"
    [[ "$output" == *"title    Arch Silverblue 2026-06-28 09:30"* ]]
    [[ "$output" == *"sort-key silverblue"* ]]
    [[ "$output" == *"version  20260628-093000"* ]]
    [[ "$output" == *"linux    /silverblue/root-20260628-093000/vmlinuz-linux"* ]]
    [[ "$output" == *"initrd   /silverblue/root-20260628-093000/intel-ucode.img"* ]]
    [[ "$output" == *"options  root=UUID=POOLUUID rootflags=subvol=root-20260628-093000 rootfstype=btrfs rw"* ]]
}

@test "sdboot_initrd_list orders microcode first and skips fallback" {
    mkdir -p "$TMP/boot"
    : > "$TMP/boot/initramfs-linux.img"
    : > "$TMP/boot/initramfs-linux-fallback.img"
    : > "$TMP/boot/intel-ucode.img"
    run sdboot_initrd_list "$TMP/boot"
    [ "${lines[0]}" = "intel-ucode.img" ]
    [ "${lines[1]}" = "initramfs-linux.img" ]
    [ "${#lines[@]}" -eq 2 ]
}

@test "sdboot_register_entry copies kernels and writes a counted entry" {
    mkdir -p "$TMP/src/boot" "$TMP/efi"
    : > "$TMP/src/boot/vmlinuz-linux"
    : > "$TMP/src/boot/initramfs-linux.img"
    : > "$TMP/src/boot/intel-ucode.img"
    sdboot_register_entry "root-X" "Label X" "UUID1" "$TMP/src/boot" "$TMP/efi" 3
    [ -f "$TMP/efi/silverblue/root-X/vmlinuz-linux" ]
    [ -f "$TMP/efi/silverblue/root-X/initramfs-linux.img" ]
    [ -f "$TMP/efi/loader/entries/root-X+3.conf" ]
    grep -q "rootflags=subvol=root-X" "$TMP/efi/loader/entries/root-X+3.conf"
}

@test "grub_render_menuentry references the subvol's /boot via Btrfs" {
    run grub_render_menuentry "root-X" "Label X" "UUID1"
    [[ "$output" == *"menuentry 'Label X' --id root-X"* ]]
    [[ "$output" == *"/root-X/boot/vmlinuz-linux"* ]]
    [[ "$output" == *"rootflags=subvol=root-X"* ]]
    [[ "$output" == *"search --no-floppy --fs-uuid --set=root UUID1"* ]]
}

@test "grub_render_header arms recordfail with a finite timeout" {
    run grub_render_header 5
    # recordfail must never hold the menu forever — an unattended machine has to keep
    # booting the default (the staged root, or saved_entry after a failed try).
    [[ "$output" != *"timeout=-1"* ]]
    [[ "$output" == *"set recordfail=1"* ]]
    [[ "$output" == *'set default="${next_entry}"'* ]]
}

@test "grub_set_next writes next_entry via the grub-editenv shim" {
    export SB_MOCK_LOG="$TMP/mock.log"
    GRUB_EDITENV="$SB_REPO/tests/unit/mocks/grub-editenv"
    mkdir -p "$TMP/efi/grub"
    grub_set_next "root-X" "$TMP/efi"
    grep -q "set next_entry=root-X" "$SB_MOCK_LOG"
}
