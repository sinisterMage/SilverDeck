#!/usr/bin/env bats
# Unattended-install additions: gather_answers_from_env (GUI frontend contract),
# lock_root_password, configure_plymouth, and the configurable bootloader timeouts.

load helper

setup() {
    TMP=$(mktemp -d)
    export SB_MOCK_LOG="$TMP/mock.log"
    export ARCH_CHROOT="$SB_REPO/tests/unit/mocks/arch-chroot"
}

teardown() { rm -rf "$TMP"; }

# Source the interactive frontend for its functions (the BASH_SOURCE guard keeps main()
# from running). It sources distro.conf + install-lib.sh itself via the env overrides.
load_frontend() {
    DISTRO_CONF="$SB_REPO/config/distro.conf"
    OSRELEASE_IN="$SB_REPO/config/os-release.in"
    SB_INSTALL_LIB="$SB_REPO/src/installer/install-lib.sh"
    export DISTRO_CONF OSRELEASE_IN SB_INSTALL_LIB
    # shellcheck source=/dev/null
    source "$SB_REPO/src/installer/silverblue-install"
    # Hermetic stand-ins for the environment-dependent pieces: the disk list normally
    # comes from lsblk, and timezone/locale validation from files the test host may lack.
    list_disks() { printf '/dev/vda|12G|\n/dev/nvme0n1|931.5G|Samsung SSD 980\n'; }
    validate_timezone() { return 0; }
    validate_locale() { return 0; }
}

@test "validate_locale matches field 1 of SUPPORTED without a pipe" {
    load_frontend
    # Real validator (load_frontend stubs it for the other tests): point it at a
    # fixture and re-source just that function's behavior via the env override.
    unset -f validate_locale
    # shellcheck source=/dev/null
    source <(sed -n '/^validate_locale()/,/^}/p' "$SB_REPO/src/installer/silverblue-install")
    printf 'aa_DJ.UTF-8 UTF-8\nen_US.UTF-8 UTF-8\nen_US ISO-8859-1\n' > "$TMP/SUPPORTED"
    SB_I18N_SUPPORTED="$TMP/SUPPORTED"
    set -o pipefail
    validate_locale "en_US.UTF-8"
    validate_locale "en_US"
    run validate_locale "xx_XX.BOGUS"
    [ "$status" -ne 0 ]
    run validate_locale ""
    [ "$status" -ne 0 ]
    # Missing file = accept anything (live ISOs without glibc's list).
    SB_I18N_SUPPORTED="$TMP/nope"
    validate_locale "whatever"
}

@test "gather_answers_from_env requires SB_INST_DISK" {
    load_frontend
    unset SB_INST_DISK
    run gather_answers_from_env
    [ "$status" -ne 0 ]
    [[ "$output" == *"SB_INST_DISK is required"* ]]
}

@test "gather_answers_from_env rejects a disk not in the installable list" {
    load_frontend
    SB_INST_DISK=/dev/sdz
    run gather_answers_from_env
    [ "$status" -ne 0 ]
    [[ "$output" == *"not an installable disk"* ]]
}

@test "gather_answers_from_env fills defaults from distro.conf" {
    load_frontend
    SB_INST_DISK=/dev/vda
    SB_INST_MICROCODE=none
    gather_answers_from_env
    [ "$SB_INST_HOSTNAME" = "$HOSTNAME" ]
    [ "$SB_INST_TIMEZONE" = "$TIMEZONE" ]
    [ "$SB_INST_LOCALE" = "$LOCALE" ]
    [ "$SB_INST_BOOTLOADER" = "$BOOTLOADER" ]
    [ "$SB_INST_FIRMWARE" = yes ]
    [ "$SB_INST_NETWORK" = networkmanager ]
    [ "$SB_INST_LOCK_ROOT" = "$INSTALL_LOCK_ROOT" ]
    [ -z "$SB_INST_USERNAME" ]
}

@test "gather_answers_from_env validates the hostname override" {
    load_frontend
    SB_INST_DISK=/dev/vda
    SB_INST_HOSTNAME="-bad-"
    run gather_answers_from_env
    [ "$status" -ne 0 ]
    [[ "$output" == *"invalid hostname"* ]]
}

@test "gather_answers_from_env without lock-root demands a root password" {
    load_frontend
    SB_INST_DISK=/dev/vda
    SB_INST_MICROCODE=none
    SB_INST_LOCK_ROOT=0
    unset SB_INST_ROOT_PASSWORD
    run gather_answers_from_env
    [ "$status" -ne 0 ]
    [[ "$output" == *"SB_INST_ROOT_PASSWORD is required"* ]]
}

@test "gather_answers_from_env builds the package list like the interactive path" {
    load_frontend
    SB_INST_DISK=/dev/vda
    SB_INST_MICROCODE=intel-ucode
    SB_INST_BOOTLOADER=grub
    gather_answers_from_env
    local pkgs=" ${SB_INST_PKGS[*]} "
    [[ "$pkgs" == *" ${PKGS_BASE[0]} "* ]]
    [[ "$pkgs" == *" intel-ucode "* ]]
    [[ "$pkgs" == *" linux-firmware "* ]]
    [[ "$pkgs" == *" grub "* ]]
    [[ "$pkgs" != *" sudo "* ]]
}

@test "lock_root_password locks root via chroot" {
    load_installer_lib
    lock_root_password "$TMP"
    grep -q "arch-chroot $TMP passwd -l root" "$SB_MOCK_LOG"
}

@test "configure_plymouth is a no-op when the target lacks plymouth" {
    load_installer_lib
    mkdir -p "$TMP/etc"
    printf 'HOOKS=(base udev autodetect modconf block filesystems fsck)\n' > "$TMP/etc/mkinitcpio.conf"
    configure_plymouth "$TMP"
    ! grep -q plymouth "$TMP/etc/mkinitcpio.conf"
    [ ! -s "$SB_MOCK_LOG" ]
}

@test "configure_plymouth inserts the hook after udev and sets the theme" {
    load_installer_lib
    PLYMOUTH_THEME=silverdeck
    mkdir -p "$TMP/etc" "$TMP/usr/bin"
    printf '#!/bin/true\n' > "$TMP/usr/bin/plymouth-set-default-theme"
    chmod +x "$TMP/usr/bin/plymouth-set-default-theme"
    printf 'HOOKS=(base udev autodetect modconf block filesystems fsck)\n' > "$TMP/etc/mkinitcpio.conf"
    configure_plymouth "$TMP"
    grep -q '^HOOKS=(base udev plymouth autodetect modconf block filesystems fsck)$' \
        "$TMP/etc/mkinitcpio.conf"
    grep -q "arch-chroot $TMP plymouth-set-default-theme silverdeck" "$SB_MOCK_LOG"
}

@test "configure_plymouth does not duplicate an existing hook" {
    load_installer_lib
    PLYMOUTH_THEME=silverdeck
    mkdir -p "$TMP/etc" "$TMP/usr/bin"
    printf '#!/bin/true\n' > "$TMP/usr/bin/plymouth-set-default-theme"
    chmod +x "$TMP/usr/bin/plymouth-set-default-theme"
    printf 'HOOKS=(base udev plymouth autodetect block filesystems)\n' > "$TMP/etc/mkinitcpio.conf"
    configure_plymouth "$TMP"
    [ "$(grep -o plymouth "$TMP/etc/mkinitcpio.conf" | wc -l)" -eq 1 ]
}

# Minimal target tree + PATH-mocked bootctl so install_sdboot runs end to end.
sdboot_fixture() {
    mkdir -p "$TMP/mnt/boot" "$TMP/mnt/efi/EFI/systemd"
    printf 'kernel\n' > "$TMP/mnt/boot/vmlinuz-linux"
    printf 'initrd\n' > "$TMP/mnt/boot/initramfs-linux.img"
    printf 'sdboot\n' > "$TMP/mnt/efi/EFI/systemd/systemd-bootx64.efi"
    export PATH="$SB_REPO/tests/unit/mocks:$PATH"
    ESP_SUBDIR=silverdeck SORT_KEY=silverdeck DISTRO_NAME=SilverDeck
}

@test "install_sdboot defaults to a 3s menu timeout" {
    load_installer_lib
    sdboot_fixture
    unset SDBOOT_TIMEOUT
    install_sdboot "$TMP/mnt" root-20260101-000000 UUID-TEST ""
    grep -q '^timeout 3$' "$TMP/mnt/efi/loader/loader.conf"
}

@test "install_sdboot honors SDBOOT_TIMEOUT=0 (hidden menu)" {
    load_installer_lib
    sdboot_fixture
    SDBOOT_TIMEOUT=0
    install_sdboot "$TMP/mnt" root-20260101-000000 UUID-TEST ""
    grep -q '^timeout 0$' "$TMP/mnt/efi/loader/loader.conf"
}

@test "install_grub renders GRUB_TIMEOUT and keeps the recordfail escape" {
    load_installer_lib
    mkdir -p "$TMP/mnt/efi"
    export PATH="$SB_REPO/tests/unit/mocks:$PATH"
    BIN_PREFIX=silverdeck DISTRO_NAME=SilverDeck GRUB_TIMEOUT=0
    install_grub "$TMP/mnt" root-20260101-000000 UUID-TEST ""
    grep -q 'set timeout=0; fi' "$TMP/mnt/efi/grub/grub.cfg"
    grep -q 'set timeout=10' "$TMP/mnt/efi/grub/grub.cfg"
}
