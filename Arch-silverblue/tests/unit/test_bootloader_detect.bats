#!/usr/bin/env bats
# Runtime bootloader detection against fixture ESP layouts.

load helper

setup() {
    load_engine
    TMP="$(mktemp -d)"
}

teardown() { rm -rf "$TMP"; }

@test "detects systemd-boot via EFI/systemd/systemd-bootx64.efi" {
    mkdir -p "$TMP/EFI/systemd"
    : > "$TMP/EFI/systemd/systemd-bootx64.efi"
    run detect_bootloader "$TMP"
    [ "$status" -eq 0 ]
    [ "$output" = "systemd-boot" ]
}

@test "detects systemd-boot via BOOTX64.EFI + loader.conf" {
    mkdir -p "$TMP/EFI/BOOT" "$TMP/loader"
    : > "$TMP/EFI/BOOT/BOOTX64.EFI"
    : > "$TMP/loader/loader.conf"
    run detect_bootloader "$TMP"
    [ "$output" = "systemd-boot" ]
}

@test "detects grub via grub/grub.cfg" {
    mkdir -p "$TMP/grub"
    : > "$TMP/grub/grub.cfg"
    run detect_bootloader "$TMP"
    [ "$output" = "grub" ]
}

@test "prefers systemd-boot when both are present" {
    mkdir -p "$TMP/EFI/systemd" "$TMP/grub"
    : > "$TMP/EFI/systemd/systemd-bootx64.efi"
    : > "$TMP/grub/grub.cfg"
    run detect_bootloader "$TMP"
    [ "$output" = "systemd-boot" ]
}

@test "fails when neither bootloader is present" {
    run detect_bootloader "$TMP"
    [ "$status" -ne 0 ]
}
