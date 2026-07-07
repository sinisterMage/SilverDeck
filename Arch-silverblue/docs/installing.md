# Installing on real hardware

Arch Silverblue ships a **minimal interactive installer**: plain prompts and numbered menus on
the console — no GUI, no dialog boxes, nothing preselected beyond sane defaults. It asks
everything upfront, shows a summary, and only touches the disk after you type `ERASE`.

## 1. Get the ISO

Download the latest ISO and checksums from
[GitHub Releases](https://github.com/sinisterMage/Arch-silverblue/releases/latest), then verify:

```bash
sha256sum -c SHA256SUMS
```

(You can also build it yourself: `make build-iso` → `iso/output/*.iso`. CI builds on every push
to `main` are available as workflow artifacts.)

## 2. Write it to a USB stick

```bash
# ALL DATA ON THE STICK IS LOST. Replace /dev/sdX with your USB device.
cp path/to/silverblue-*.iso /dev/sdX && sync
```

`dd if=... of=/dev/sdX bs=4M oflag=sync` works too, as do tools like Ventoy or GNOME Disks.

## 3. Boot it — UEFI only

Boot the stick in **UEFI mode**. Secure Boot is not supported — disable it in firmware setup.
If the installer reports "booted in BIOS mode", switch your firmware from Legacy/CSM to UEFI.

## 4. Run the installer

Log in lands you in a root shell. Start the installer:

```bash
silverblue-install
```

It checks UEFI mode and network connectivity first (installation pacstraps from the Arch
mirrors — for Wi-Fi, bring the link up with `iwctl` before/when prompted, Ethernet with DHCP
just works), then asks, in order:

| Prompt | Notes | Default |
|---|---|---|
| Target disk | Numbered menu; the live USB itself is excluded | first disk |
| Hostname | | `silverblue` |
| Timezone | e.g. `Europe/Amsterdam` | `UTC` |
| Locale | validated against glibc's list | `en_US.UTF-8` |
| Console keymap | empty keeps the kernel default | empty |
| Bootloader | `systemd-boot` (primary, CI-validated) or `grub` | `systemd-boot` |
| CPU microcode | detected from `/proc/cpuinfo` (`intel-ucode`/`amd-ucode`) | yes |
| linux-firmware | needed on most real hardware; skip only in VMs | yes |
| Network stack | `none` / `systemd-networkd` (DHCP, ships with systemd) / `NetworkManager` | `systemd-networkd` |
| Root password | required — the installed system has no passwordless accounts | — |
| Admin user | optional; created in `wheel` with a sudoers drop-in (installs `sudo`) | none |

After the summary, type `ERASE` to proceed. The installer partitions the disk (512 MB ESP +
Btrfs), creates the initial `root-<timestamp>` and `@home` subvolumes, pacstraps the base
system plus your choices, installs the update engine and health-check units, sets up the
bootloader, and offers a reboot. The resulting on-disk layout is exactly the one
[`update-flow.md`](update-flow.md) describes.

If a step fails, the installer reports the failing line, unmounts the target, and leaves you
in the live shell — nothing is half-mounted and the log is on your screen.

## 5. First boot

Log in as root (or your admin user) and try the update flow:

```bash
silverblue-update --dry-run   # show the plan
silverblue-update             # stage an updated root for the next boot
```

The first boot is automatically health-checked and marked good by
`silverblue-mark-good.service`; a failed boot of a *staged update* rolls back to the previous
root on its own.

## Troubleshooting

- **"booted in BIOS mode"** — enable UEFI (disable CSM/Legacy) in firmware setup; the ISO and
  the installed system are UEFI-only.
- **"no outbound network"** — Ethernet: plug in and retry (DHCP is automatic on the live ISO).
  Wi-Fi: `iwctl station wlan0 connect <SSID>`, then retry at the prompt.
- **Disk not listed** — the installer only offers whole disks (not partitions) and hides the
  live USB, optical, loop, and zram devices. Check `lsblk -d`.
- **Disk size** — keep at least ~12 GB; the ESP holds up to 3 snapshots × ~80–120 MB of
  kernels, the Btrfs pool holds up to 3 root snapshots.

## Scope — deliberately not included

No LUKS encryption, no swap setup, no partitioning schemes beyond ESP + single Btrfs pool, no
GUI, no package-set choices beyond the prompts above. Arch Silverblue stays unopinionated:
everything else is a normal `pacman` command away after the first boot.
