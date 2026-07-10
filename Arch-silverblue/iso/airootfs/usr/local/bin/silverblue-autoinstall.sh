#!/usr/bin/env bash
#
# silverblue-autoinstall.sh — unattended Arch Silverblue install onto /dev/vda.
#
# Runs in the live ISO, gated by the QEMU fw_cfg blob opt/silverblue/scenario=install (see
# silverblue-autoinstall.service). It partitions the disk (ESP + Btrfs), creates the initial
# root-<TS> and @home subvolumes, pacstraps a minimal system + the Silverblue tools, installs
# the requested bootloader with an initial boot entry, configures serial-console autologin so
# the QEMU harness can drive the booted system, then powers off. Progress is reported with
# SILVERBLUE-* markers on the serial console.
#
# The generic install steps live in install-lib.sh (shared with the interactive installer);
# only the test-appliance behavior stays here: the fw_cfg gate, the hardcoded /dev/vda, the
# passwordless root, the target autologin drop-in, the local test repo, and the poweroff.
#
# It is intentionally conservative: it only proceeds when the scenario blob says "install".

fwc() { cat "/sys/firmware/qemu_fw_cfg/by_name/$1/raw" 2>/dev/null || true; }

# Load the derived-distro configuration that build.sh shipped into the ISO. A fixed,
# id-independent path so this script can find it without already knowing the distro id.
DISTRO_CONF=${DISTRO_CONF:-/usr/local/share/distro/distro.conf}
OSRELEASE_IN=${OSRELEASE_IN:-/usr/local/share/distro/os-release.in}
# shellcheck source=../../../../../config/distro.conf
source "$DISTRO_CONF" || { echo "error: cannot load $DISTRO_CONF" >&2; exit 1; }

# Shared install logic (build.sh ships it at this fixed path on the ISO).
SB_INSTALL_LIB=${SB_INSTALL_LIB:-/usr/local/lib/installer/install-lib.sh}
# shellcheck source=../../../../../src/installer/install-lib.sh
source "$SB_INSTALL_LIB" || { echo "error: cannot load $SB_INSTALL_LIB" >&2; exit 1; }

configure_pacman_repo() {
    local mnt=$1 net=$2
    cat >> "$mnt/etc/pacman.conf" <<'EOF'

[silverblue-local]
SigLevel = Optional TrustAll
Server = file:///opt/silverblue/localrepo
EOF
    if [[ "$net" == 1 ]]; then
        # Networked install: add the derivative's extra repos (if any) to the target.
        append_extra_repos "$mnt"
    else
        # Hermetic update test: disable remote repos so `pacman -Syu` only needs the offline
        # file:// repo. Comment each remote section header and its Include/Server lines.
        awk '
            /^\[(core|extra|multilib)\]/ { print "#" $0; skip=1; next }
            skip==1 && /^(Include|Server)/ { print "#" $0; next }
            /^\[/ { skip=0 }
            { print }
        ' "$mnt/etc/pacman.conf" > "$mnt/etc/pacman.conf.new"
        mv "$mnt/etc/pacman.conf.new" "$mnt/etc/pacman.conf"
    fi
}

main() {
    local scenario net bootloader disk esp rootpart ts snap pool_uuid
    local -a pkgs
    # Config comes from the environment (the QEMU harness drives this over the autologin
    # shell), falling back to QEMU fw_cfg blobs for the service-driven path.
    scenario=${SB_SCENARIO:-$(fwc opt/silverblue/scenario)}
    if [[ "$scenario" != install ]]; then
        marker "SILVERBLUE-INSTALL-SKIP scenario=${scenario:-none}"
        exit 0
    fi
    net=${SB_NET:-$(fwc opt/silverblue/net)}; net=${net:-0}
    bootloader=${SB_BOOTLOADER:-$(fwc opt/silverblue/bootloader)}; bootloader=${bootloader:-$BOOTLOADER}
    disk=/dev/vda

    # shellcheck disable=SC2154   # LINENO is provided by bash
    trap 'marker "SILVERBLUE-INSTALL-FAIL line=$LINENO"; sync; poweroff -f' ERR

    ts=$(date +%Y%m%d-%H%M%S)
    snap="root-$ts"
    marker "SILVERBLUE-INSTALL-START disk=$disk snap=$snap bootloader=$bootloader net=$net"

    partition_disk "$disk"
    esp=$(partition_path "$disk" 1)
    rootpart=$(partition_path "$disk" 2)
    format_partitions "$esp" "$rootpart"
    create_subvolumes "$rootpart" "$snap"
    mount_target "$rootpart" "$esp" "$snap"

    # Wait for outbound network (pacstrap needs the mirrors).
    marker "SILVERBLUE-INSTALL-NETWAIT"
    wait_network

    # grub must come in via pacstrap (live pacman.conf + network): by bootloader-install
    # time the hermetic path has already disabled the target's remote repos.
    pkgs=("${PKGS_BASE[@]}")
    if [[ "$bootloader" == grub ]]; then pkgs+=(grub); fi
    enable_live_multilib
    enable_live_local_repo
    run_pacstrap /mnt "${pkgs[@]}"
    write_fstab /mnt
    configure_target_system /mnt "$HOSTNAME" "$TIMEZONE" "$LOCALE" "$KEYMAP"
    write_os_release /mnt

    # Test appliance: no root password — the harness drives an autologin shell.
    arch-chroot /mnt passwd -d root

    configure_plymouth /mnt
    configure_initramfs /mnt

    # Serial-console autologin on the target so the harness can drive it.
    install -Dm0644 /dev/stdin \
        /mnt/etc/systemd/system/serial-getty@ttyS0.service.d/autologin.conf <<'EOF'
[Service]
ExecStart=
ExecStart=-/sbin/agetty -o '-p -- \\u' --autologin root --keep-baud 115200,38400,9600 - $TERM
EOF

    install_target_tools /mnt
    install_local_repo /mnt

    # --- Local repo + marker v1; configure pacman for the update test --------------------
    cp -a /opt/silverblue /mnt/opt/silverblue
    arch-chroot /mnt pacman -U --noconfirm \
        /opt/silverblue/install/silverblue-marker-1-1-any.pkg.tar.zst
    configure_pacman_repo /mnt "$net"
    enable_distro_services /mnt

    # --- Bootloader ----------------------------------------------------------------------
    # The serial console comes first; KERNEL_OPTS_EXTRA (quiet/splash on SilverDeck) rides
    # along so the QEMU-installed target exercises the exact boot the GUI installer produces.
    local extra_opts="console=ttyS0,115200 console=tty0"
    if [[ -n "${KERNEL_OPTS_EXTRA:-}" ]]; then
        extra_opts="$extra_opts $KERNEL_OPTS_EXTRA"
    fi
    pool_uuid=$(blkid -s UUID -o value "$rootpart")
    if [[ "$bootloader" == grub ]]; then
        install_grub /mnt "$snap" "$pool_uuid" "$extra_opts"
    else
        install_sdboot /mnt "$snap" "$pool_uuid" "$extra_opts"
    fi

    sync
    umount -R /mnt
    marker "SILVERBLUE-INSTALL-OK snap=$snap uuid=$pool_uuid bootloader=$bootloader"
    poweroff -f
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    # -E so the ERR trap (FAIL marker + poweroff) also fires for failures inside the
    # sourced library's functions, not only for commands directly in main().
    set -Eeuo pipefail
    main "$@"
fi
