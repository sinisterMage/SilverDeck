# shellcheck shell=bash
#
# install-lib.sh — shared install logic for Arch Silverblue.
#
# Sourced by both install frontends:
#   * iso/airootfs/usr/local/bin/silverblue-autoinstall.sh (unattended QEMU test appliance)
#   * src/installer/silverblue-install                     (interactive plain-prompt installer)
#
# It only defines functions (no set -e, no main) and is safe to source from bats for unit
# tests. Callers source config/distro.conf first: the effectful functions read the config
# globals (FS_LABEL, ESP_LABEL, DISTRO_*, BIN_PREFIX, UNIT_PREFIX, LIB_DIR, ...).
# Test-only behavior (fw_cfg gating, /dev/vda, passwordless root, the local test repo)
# stays in the autoinstaller; nothing here is QEMU-specific.

# Dependency-injected external commands (overridable so tests can mock them).
: "${LSBLK:=lsblk}"
: "${FINDMNT:=findmnt}"
: "${BLKID:=blkid}"
: "${SGDISK:=sgdisk}"
: "${ARCH_CHROOT:=arch-chroot}"
: "${PACSTRAP:=pacstrap}"

log() { printf '==> %s\n' "$*" >&2; }
err() { printf 'error: %s\n' "$*" >&2; }
die() { err "$*"; exit 1; }

# Progress markers on stdout. Uppercase SILVERBLUE-* markers are a contract with the QEMU
# harness (grepped literally, never renamed by iso/build.sh's render()).
marker() { printf '%s\n' "$*"; }

# --- Pure helpers (unit-tested; no IO) ------------------------------------------------------

# Escape a string for the replacement side of a sed 's|...|...|' command.
esc() { printf '%s' "$1" | sed -e 's/[\\&|]/\\&/g'; }

# RFC-1123 single-label host name.
validate_hostname() {
    [[ "$1" =~ ^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$ ]]
}

# useradd-compatible user name.
validate_username() {
    [[ "$1" =~ ^[a-z_][a-z0-9_-]{0,31}$ ]]
}

# Print the microcode package for the CPU described by the /proc/cpuinfo text in $1
# (nothing when the vendor is not recognized).
detect_microcode() {
    local cpuinfo=$1
    if [[ "$cpuinfo" == *GenuineIntel* ]]; then
        printf 'intel-ucode\n'
    elif [[ "$cpuinfo" == *AuthenticAMD* ]]; then
        printf 'amd-ucode\n'
    fi
}

# Filter `lsblk -dn -P -o PATH,TYPE,SIZE,MODEL` output ($1) down to installable disks,
# dropping the live-medium disk ($2, may be empty) and loop/rom/zram/ram/floppy devices.
# Prints one 'PATH|SIZE|MODEL' line per candidate.
filter_disks() {
    local lsblk_text=$1 live_disk=${2:-}
    local line path type size model
    while IFS= read -r line; do
        [[ -n "$line" ]] || continue
        path=""; type=""; size=""; model=""
        [[ "$line" =~ PATH=\"([^\"]*)\" ]] && path=${BASH_REMATCH[1]}
        [[ "$line" =~ TYPE=\"([^\"]*)\" ]] && type=${BASH_REMATCH[1]}
        [[ "$line" =~ SIZE=\"([^\"]*)\" ]] && size=${BASH_REMATCH[1]}
        [[ "$line" =~ MODEL=\"([^\"]*)\" ]] && model=${BASH_REMATCH[1]}
        [[ "$type" == disk && -n "$path" ]] || continue
        [[ "$path" == "$live_disk" ]] && continue
        [[ "$path" =~ ^/dev/(loop|ram|zram|sr|fd)[0-9]*$ ]] && continue
        printf '%s|%s|%s\n' "$path" "$size" "$model"
    done <<< "$lsblk_text"
    return 0
}

# Partition device node: disks whose name ends in a digit get a 'p' separator
# (/dev/nvme0n1 -> /dev/nvme0n1p1, /dev/vda -> /dev/vda1).
partition_path() {
    local disk=$1 n=$2
    if [[ "$disk" =~ [0-9]$ ]]; then
        printf '%sp%s\n' "$disk" "$n"
    else
        printf '%s%s\n' "$disk" "$n"
    fi
}

# Kernel console= options for the tty the installer runs on ($1, as printed by tty(1)).
# A serial-console install carries its console to the installed system's boot entry;
# anything else adds nothing.
console_opts_for_tty() {
    local tty=$1
    if [[ "$tty" =~ ^/dev/(ttyS[0-9]+)$ ]]; then
        printf 'console=%s,115200 console=tty0\n' "${BASH_REMATCH[1]}"
    fi
}

# Render the pre-install confirmation summary from the SB_INST_* globals set by the
# interactive frontend's gather_answers().
render_summary() {
    printf 'Installation summary\n'
    printf '  Disk           : %s\n' "${SB_INST_DISK:-}"
    printf '  Hostname       : %s\n' "${SB_INST_HOSTNAME:-}"
    printf '  Timezone       : %s\n' "${SB_INST_TIMEZONE:-}"
    printf '  Locale         : %s\n' "${SB_INST_LOCALE:-}"
    printf '  Keymap         : %s\n' "${SB_INST_KEYMAP:-(kernel default)}"
    printf '  Bootloader     : %s\n' "${SB_INST_BOOTLOADER:-}"
    printf '  Microcode      : %s\n' "${SB_INST_MICROCODE:-none}"
    printf '  linux-firmware : %s\n' "${SB_INST_FIRMWARE:-no}"
    printf '  Network        : %s\n' "${SB_INST_NETWORK:-none}"
    printf '  Admin user     : %s\n' "${SB_INST_USERNAME:-(none)}"
    printf '  Packages       : %s\n' "${SB_INST_PKGS[*]:-}"
}

# --- Prompt helpers (unit-tested via piped stdin) -------------------------------------------
# Prompt text goes to stderr and the result to stdout, so callers can capture answers with
# $(...) while the user still sees the prompts. When SB_INSTALL_MARKERS=1, each prompt is
# preceded by a 'SILVERBLUE-INSTALL-PROMPT key=<key>' line on stderr; the QEMU harness keys
# its scripted answers off these markers instead of the human-readable prompt text.

prompt_marker() {
    [[ "${SB_INSTALL_MARKERS:-0}" == 1 ]] || return 0
    printf 'SILVERBLUE-INSTALL-PROMPT key=%s\n' "$1" >&2
}

# Read one line into the caller's $reply; tolerate a final unterminated line, fail on EOF.
read_reply() {
    IFS= read -r reply && return 0
    [[ -n "$reply" ]] || { err "unexpected end of input"; return 1; }
}

read_reply_secret() {
    IFS= read -rs reply && return 0
    [[ -n "$reply" ]] || { err "unexpected end of input"; return 1; }
}

# ask KEY PROMPT DEFAULT [VALIDATOR] — free-text prompt. Empty input takes the default;
# re-prompts until VALIDATOR (a function name) accepts the value.
ask() {
    local key=$1 prompt=$2 default=$3 validator=${4:-} reply
    while true; do
        prompt_marker "$key"
        if [[ -n "$default" ]]; then
            printf '%s [%s]: ' "$prompt" "$default" >&2
        else
            printf '%s: ' "$prompt" >&2
        fi
        read_reply || return 1
        reply=${reply:-$default}
        if [[ -z "$validator" ]] || "$validator" "$reply"; then
            printf '%s\n' "$reply"
            return 0
        fi
        printf 'invalid value: %s\n' "$reply" >&2
    done
}

# ask_yesno KEY PROMPT DEFAULT(y|n) — returns 0 for yes, 1 for no.
ask_yesno() {
    local key=$1 prompt=$2 default=$3 reply hint="y/N"
    [[ "$default" == y ]] && hint="Y/n"
    while true; do
        prompt_marker "$key"
        printf '%s [%s]: ' "$prompt" "$hint" >&2
        read_reply || return 1
        reply=${reply:-$default}
        case "${reply,,}" in
            y|yes) return 0 ;;
            n|no)  return 1 ;;
        esac
        printf 'please answer y or n\n' >&2
    done
}

# choose KEY PROMPT DEFAULT ITEM... — numbered menu; prints the chosen item to stdout.
# Empty input selects DEFAULT (an item value, not an index).
choose() {
    local key=$1 prompt=$2 default=$3
    shift 3
    local items=("$@") i reply
    while true; do
        prompt_marker "$key"
        printf '%s\n' "$prompt" >&2
        for i in "${!items[@]}"; do
            printf '  %d) %s\n' "$((i + 1))" "${items[$i]}" >&2
        done
        printf 'Select [1-%d] (default: %s): ' "${#items[@]}" "$default" >&2
        read_reply || return 1
        if [[ -z "$reply" ]]; then
            printf '%s\n' "$default"
            return 0
        fi
        if [[ "$reply" =~ ^[0-9]+$ ]] && (( reply >= 1 && reply <= ${#items[@]} )); then
            printf '%s\n' "${items[$((reply - 1))]}"
            return 0
        fi
        printf 'invalid selection: %s\n' "$reply" >&2
    done
}

# ask_secret KEY PROMPT — read a password twice without echo (markers KEY and KEY-confirm);
# loops until the entries are non-empty and match.
ask_secret() {
    local key=$1 prompt=$2 first reply
    while true; do
        prompt_marker "$key"
        printf '%s: ' "$prompt" >&2
        read_reply_secret || return 1
        printf '\n' >&2
        first=$reply
        prompt_marker "$key-confirm"
        printf '%s (again): ' "$prompt" >&2
        read_reply_secret || return 1
        printf '\n' >&2
        if [[ -z "$first" ]]; then
            printf 'password must not be empty\n' >&2
            continue
        fi
        if [[ "$first" != "$reply" ]]; then
            printf 'passwords do not match\n' >&2
            continue
        fi
        printf '%s\n' "$first"
        return 0
    done
}

# --- Effectful install steps (exercised by the QEMU integration test) -----------------------
# The canonical sequence both frontends run; see docs/update-flow.md for the resulting
# on-disk layout. All of these expect config/distro.conf to have been sourced.

# One outbound connectivity probe (mirrors reachable / DNS up).
check_network() {
    curl -fsS --max-time 5 https://geo.mirror.pkgbuild.com/ >/dev/null 2>&1 \
        || ping -c1 -W2 8.8.8.8 >/dev/null 2>&1
}

# Wait up to ~60s for outbound network (slow DHCP); proceeds either way — pacstrap gives
# the definitive error if the mirrors really are unreachable.
wait_network() {
    local _
    for _ in $(seq 1 30); do
        check_network && return 0
        sleep 2
    done
    return 0
}

# Partition $1: p1 ESP (FAT32, 512M), p2 Btrfs (rest).
partition_disk() {
    local disk=$1
    wipefs -a "$disk"
    "$SGDISK" -Z "$disk"
    "$SGDISK" -n1:0:+512M -t1:ef00 -c1:EFI "$disk"
    "$SGDISK" -n2:0:0     -t2:8300 -c2:"$FS_LABEL" "$disk"
    partprobe "$disk" 2>/dev/null || true
    udevadm settle || true
    sleep 1
}

format_partitions() {
    local esp=$1 rootpart=$2
    mkfs.fat -F32 -n "$ESP_LABEL" "$esp"
    mkfs.btrfs -f -L "$FS_LABEL" "$rootpart"
}

# Create the initial root-<TS> subvolume ($2) and @home on $1.
create_subvolumes() {
    local rootpart=$1 snap=$2
    mount "$rootpart" /mnt
    btrfs subvolume create "/mnt/$snap"
    btrfs subvolume create /mnt/@home
    umount /mnt
}

# Mount the target tree at /mnt (root subvol + @home + ESP).
mount_target() {
    local rootpart=$1 esp=$2 snap=$3
    mount -o "subvol=$snap,compress=zstd" "$rootpart" /mnt
    mkdir -p /mnt/efi /mnt/home
    mount -o subvol=@home "$rootpart" /mnt/home
    mount "$esp" /mnt/efi
}

run_pacstrap() {
    local mnt=$1
    shift
    "$PACSTRAP" -K "$mnt" "$@"
}

# fstab: omit the '/' line — the root subvolume comes from the kernel cmdline rootflags.
write_fstab() {
    local mnt=$1
    genfstab -U "$mnt" | awk '$2 != "/"' > "$mnt/etc/fstab"
}

configure_target_system() {
    local mnt=$1 hostname=$2 timezone=$3 locale=$4 keymap=$5
    printf '%s\n' "$hostname" > "$mnt/etc/hostname"
    ln -sf "/usr/share/zoneinfo/$timezone" "$mnt/etc/localtime"
    printf '%s UTF-8\n' "$locale" > "$mnt/etc/locale.gen"
    "$ARCH_CHROOT" "$mnt" locale-gen
    printf 'LANG=%s\n' "$locale" > "$mnt/etc/locale.conf"
    if [[ -n "$keymap" ]]; then printf 'KEYMAP=%s\n' "$keymap" > "$mnt/etc/vconsole.conf"; fi
}

# /etc/os-release — a regular file overrides the stock symlink to /usr/lib/os-release.
write_os_release() {
    local mnt=$1
    sed -e "s|@DISTRO_NAME@|$(esc "$DISTRO_NAME")|g" \
        -e "s|@DISTRO_ID@|$(esc "$DISTRO_ID")|g" \
        -e "s|@DISTRO_VERSION@|$(esc "$DISTRO_VERSION")|g" \
        -e "s|@DISTRO_VERSION_ID@|$(esc "$DISTRO_VERSION_ID")|g" \
        -e "s|@DISTRO_ANSI_COLOR@|$(esc "$DISTRO_ANSI_COLOR")|g" \
        -e "s|@DISTRO_HOME_URL@|$(esc "$DISTRO_HOME_URL")|g" \
        -e "s|@DISTRO_DOC_URL@|$(esc "$DISTRO_DOC_URL")|g" \
        -e "s|@DISTRO_SUPPORT_URL@|$(esc "$DISTRO_SUPPORT_URL")|g" \
        -e "s|@DISTRO_BUG_URL@|$(esc "$DISTRO_BUG_URL")|g" \
        "$OSRELEASE_IN" > "$mnt/etc/os-release"
}

# initramfs must carry btrfs (autodetect can't see it from the live medium's root).
configure_initramfs() {
    local mnt=$1
    sed -i 's/^MODULES=.*/MODULES=(btrfs)/' "$mnt/etc/mkinitcpio.conf"
    "$ARCH_CHROOT" "$mnt" mkinitcpio -P
}

# Add plymouth to the target's initramfs hooks and select the distro theme
# (PLYMOUTH_THEME in distro.conf). No-op when the target does not ship plymouth, so
# stock builds are unaffected. Must run before configure_initramfs: the plymouth
# mkinitcpio hook bakes the selected theme into the image, and configure_initramfs
# does the single mkinitcpio -P rebuild for both changes.
configure_plymouth() {
    local mnt=$1
    [[ -x "$mnt/usr/bin/plymouth-set-default-theme" ]] || return 0
    if ! grep -Eq '^HOOKS=.*[( ]plymouth[ )]' "$mnt/etc/mkinitcpio.conf"; then
        sed -i -E 's/^(HOOKS=\(.*\budev\b)/\1 plymouth/' "$mnt/etc/mkinitcpio.conf"
    fi
    if [[ -n "${PLYMOUTH_THEME:-}" ]]; then
        "$ARCH_CHROOT" "$mnt" plymouth-set-default-theme "$PLYMOUTH_THEME"
    fi
}

# Copy the distro tools (already renamed/rendered in the ISO by build.sh) into the target
# and enable the post-boot health check.
install_target_tools() {
    local mnt=$1 u
    install -Dm0755 "/usr/bin/${BIN_PREFIX}-update" "$mnt/usr/bin/${BIN_PREFIX}-update"
    mkdir -p "${mnt}${LIB_DIR}"
    cp -a "${LIB_DIR}/." "${mnt}${LIB_DIR}/"
    # Ensure the entry-point scripts are executable on the target regardless of source modes.
    chmod 0755 "${mnt}${LIB_DIR}/${UNIT_PREFIX}-mark-good.sh" \
        "${mnt}${LIB_DIR}/${UNIT_PREFIX}-rollback.sh"
    for u in "${UNIT_PREFIX}-mark-good.service" "${UNIT_PREFIX}-rollback.service" "${UNIT_PREFIX}-rollback.target"; do
        install -Dm0644 "/usr/lib/systemd/system/$u" "$mnt/usr/lib/systemd/system/$u"
    done
    install -Dm0644 "/etc/systemd/system.conf.d/${UNIT_PREFIX}-watchdog.conf" \
        "$mnt/etc/systemd/system.conf.d/${UNIT_PREFIX}-watchdog.conf"
    "$ARCH_CHROOT" "$mnt" systemctl enable "${UNIT_PREFIX}-mark-good.service"
}

# Append the derivative's extra pacman repos (if any) to the target's pacman.conf.
append_extra_repos() {
    local mnt=$1 r
    for r in "${EXTRA_REPOS[@]}"; do
        printf '\n%s\n' "$r" >> "$mnt/etc/pacman.conf"
    done
}

# --- Derivative local repo + services (no-ops on a stock build) -----------------------------
# iso/build.sh bakes a prebuilt pacman repo (the derivative's own packages, e.g. the
# SilverDeck UI/session) into the live ISO at /opt/${DISTRO_ID}/repo when the derivative
# ships one. These helpers make it usable at install time and afterwards.

# Enable [multilib] in the LIVE environment's pacman.conf when the derivative needs
# lib32 packages (ENABLE_MULTILIB=1 in distro.conf): pacstrap resolves the target's
# packages against the live config, and the live image ships multilib commented out.
# Idempotent: uncomments a commented section, else appends one.
enable_live_multilib() {
    local conf="${SB_LIVE_PACMAN_CONF:-/etc/pacman.conf}"
    [[ "${ENABLE_MULTILIB:-0}" == 1 ]] || return 0
    grep -q '^\[multilib\]' "$conf" 2>/dev/null && return 0
    sed -i '/^#\[multilib\]/,/^#Include/ s/^#//' "$conf"
    grep -q '^\[multilib\]' "$conf" 2>/dev/null && return 0
    printf '\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n' >> "$conf"
}

# Register the baked repo in the LIVE environment's pacman.conf so pacstrap can resolve
# the derivative's packages. Idempotent; no-op when the ISO carries no repo.
enable_live_local_repo() {
    local dir="/opt/${DISTRO_ID}/repo" conf="${SB_LIVE_PACMAN_CONF:-/etc/pacman.conf}"
    [[ -d "$dir" ]] || return 0
    grep -qxF "[${DISTRO_ID}]" "$conf" 2>/dev/null && return 0
    printf '\n[%s]\nSigLevel = Optional TrustAll\nServer = file://%s\n' \
        "$DISTRO_ID" "$dir" >> "$conf"
}

# Copy the baked repo onto the target (/var/lib/${DISTRO_ID}/repo) so the [<id>] entry
# the derivative ships in EXTRA_REPOS keeps resolving after install: the repo lives
# inside the root subvolume, so every snapshot the update engine creates carries it.
install_local_repo() {
    local mnt=$1 dir="/opt/${DISTRO_ID}/repo"
    [[ -d "$dir" ]] || return 0
    mkdir -p "$mnt/var/lib/${DISTRO_ID}"
    cp -a "$dir" "$mnt/var/lib/${DISTRO_ID}/repo"
}

# Enable the derivative's services (ENABLE_SERVICES in distro.conf) in the target.
# Empty/unset list = stock behavior (nothing enabled beyond the health check).
enable_distro_services() {
    local mnt=$1 svc
    [[ -n "${ENABLE_SERVICES[*]:-}" ]] || return 0
    for svc in "${ENABLE_SERVICES[@]}"; do
        "$ARCH_CHROOT" "$mnt" systemctl enable "$svc"
    done
}

set_root_password() {
    local mnt=$1 password=$2
    printf 'root:%s\n' "$password" | "$ARCH_CHROOT" "$mnt" chpasswd
}

# Lock the root account (no password logins). Used by unattended installs on kiosk
# targets where the session autologs into a regular user and admin work happens via
# sudo or a recovery environment.
lock_root_password() {
    local mnt=$1
    "$ARCH_CHROOT" "$mnt" passwd -l root
}

# Create a wheel-group admin user with sudo access (the frontend adds the sudo package to
# the pacstrap list when a user is requested).
create_admin_user() {
    local mnt=$1 user=$2 password=$3
    "$ARCH_CHROOT" "$mnt" useradd -m -G wheel -s /bin/bash "$user"
    printf '%s:%s\n' "$user" "$password" | "$ARCH_CHROOT" "$mnt" chpasswd
    printf '%%wheel ALL=(ALL:ALL) ALL\n' | install -Dm0440 /dev/stdin "$mnt/etc/sudoers.d/10-wheel"
}

# enable_network_stack MNT none|networkd|networkmanager — keep the base unopinionated:
# networkd only enables what systemd already ships; NetworkManager is opt-in (the frontend
# adds the package to the pacstrap list).
enable_network_stack() {
    local mnt=$1 mode=$2
    case "$mode" in
        networkd)
            install -Dm0644 /dev/stdin "$mnt/etc/systemd/network/20-wired.network" <<'EOF'
[Match]
Type=ether

[Network]
DHCP=yes
EOF
            "$ARCH_CHROOT" "$mnt" systemctl enable systemd-networkd.service systemd-resolved.service
            ln -sf ../run/systemd/resolve/stub-resolv.conf "$mnt/etc/resolv.conf"
            ;;
        networkmanager)
            "$ARCH_CHROOT" "$mnt" systemctl enable NetworkManager.service
            ;;
        none|"")
            ;;
        *)
            err "unknown network mode: $mode"
            return 1
            ;;
    esac
}

# Install systemd-boot with an initial boot entry for snapshot $2.
#   $1 mnt  $2 snap  $3 pool_uuid  $4 extra kernel cmdline options (may be empty)
install_sdboot() {
    local mnt=$1 snap=$2 uuid=$3 extra_opts=$4
    local efi="$mnt/efi" img
    bootctl --esp-path="$efi" install
    mkdir -p "$efi/EFI/BOOT" "$efi/$ESP_SUBDIR/$snap" "$efi/loader/entries"
    # Removable fallback so the machine boots even without persisted EFI NVRAM.
    cp "$efi/EFI/systemd/systemd-bootx64.efi" "$efi/EFI/BOOT/BOOTX64.EFI"
    cp "$mnt/boot/vmlinuz-linux" "$efi/$ESP_SUBDIR/$snap/"
    # Microcode images load before the initramfs; copy and list them first (the same
    # ordering the update engine's sdboot helpers use for every later entry).
    for img in "$mnt"/boot/*-ucode.img; do
        [[ -e "$img" ]] || continue
        cp "$img" "$efi/$ESP_SUBDIR/$snap/"
    done
    cp "$mnt/boot/initramfs-linux.img" "$efi/$ESP_SUBDIR/$snap/"
    # No explicit default= : systemd-boot then selects the newest-version entry, which is how
    # the update engine makes a freshly registered root boot next without touching the default.
    # SDBOOT_TIMEOUT=0 (distro.conf) hides the menu for a console-style boot; holding a key
    # during firmware handoff still brings it up, and boot-counting keeps demoting bad entries.
    cat > "$efi/loader/loader.conf" <<EOF
timeout ${SDBOOT_TIMEOUT:-3}
console-mode max
EOF
    {
        printf 'title    %s (initial) %s\n' "$DISTRO_NAME" "$snap"
        printf 'sort-key %s\n' "$SORT_KEY"
        printf 'version  %s\n' "${snap#root-}"
        printf 'linux    /%s/%s/vmlinuz-linux\n' "$ESP_SUBDIR" "$snap"
        for img in "$efi/$ESP_SUBDIR/$snap/"*-ucode.img; do
            [[ -e "$img" ]] || continue
            printf 'initrd   /%s/%s/%s\n' "$ESP_SUBDIR" "$snap" "${img##*/}"
        done
        printf 'initrd   /%s/%s/initramfs-linux.img\n' "$ESP_SUBDIR" "$snap"
        printf 'options  root=UUID=%s rootflags=subvol=%s rootfstype=btrfs rw%s\n' \
            "$uuid" "$snap" "${extra_opts:+ $extra_opts}"
    } > "$efi/loader/entries/$snap.conf"
}

# Install GRUB (removable layout) with the grubenv-on-ESP scheme the update engine manages.
# The grub package must already be on the target: frontends add it to the pacstrap list
# (installing it here via chroot pacman would break the hermetic test path, whose target
# has the remote repos disabled).
#   $1 mnt  $2 snap  $3 pool_uuid  $4 extra kernel cmdline options (may be empty)
install_grub() {
    local mnt=$1 snap=$2 uuid=$3 extra_opts=$4
    local efi="$mnt/efi"
    "$ARCH_CHROOT" "$mnt" grub-install --target=x86_64-efi --efi-directory=/efi \
        --boot-directory=/efi --bootloader-id=GRUB --removable
    mkdir -p "$efi/grub"
    grub-editenv "$efi/grub/grubenv" create
    grub-editenv "$efi/grub/grubenv" set "saved_entry=$snap"
    cat > "$efi/grub/grub.cfg" <<EOF
# Generated by the installer; managed by ${BIN_PREFIX}-update thereafter.
load_env --file \${prefix}/grubenv
set default="\${saved_entry}"
if [ -n "\${next_entry}" ]; then
    set default="\${next_entry}"; set next_entry=; save_env --file \${prefix}/grubenv next_entry
    set recordfail=1; save_env --file \${prefix}/grubenv recordfail
fi
if [ "\${recordfail}" = 1 ]; then set timeout=10; else set timeout=${GRUB_TIMEOUT:-3}; fi
menuentry '$DISTRO_NAME (initial) $snap' --id $snap {
    insmod btrfs
    insmod part_gpt
    search --no-floppy --fs-uuid --set=root $uuid
    linux  /$snap/boot/vmlinuz-linux root=UUID=$uuid rootflags=subvol=$snap rootfstype=btrfs rw${extra_opts:+ $extra_opts}
    initrd /$snap/boot/initramfs-linux.img
}
EOF
}
