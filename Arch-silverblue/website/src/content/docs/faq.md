---
title: FAQ
description: Frequently asked questions about Arch Silverblue — atomicity, mutability, rollback behavior, hardware support, and how it compares to snapshot tooling.
---

## Is this affiliated with Fedora Silverblue?

No. The name is a nod, but the mechanism is entirely different: Fedora
Silverblue is image-based (rpm-ostree) with a read-only `/usr`; Arch
Silverblue is plain Arch with Btrfs copy-on-write snapshots and a fully
writable root.

## Is the system immutable?

No — and that's the point. There is no read-only `/usr`, no overlayfs, no
immutable root. Atomicity applies to *updates*: each one builds a complete new
root in a snapshot and only keeps it if the next boot is healthy. Between
updates, it's ordinary writable Arch.

## Can I still use pacman and the AUR normally?

Yes. The running root is normal Arch — direct `pacman -S` installs and AUR
helpers work exactly as always and land on the live system. `silverblue-update`
is for the system upgrades you want atomicity and auto-rollback for.

## What happens to my files on rollback?

Nothing. `/home` lives in a shared `@home` subvolume that is never snapshotted
and never rolled back — it survives every update and every rollback unchanged.

## What actually triggers an automatic rollback?

Three complementary mechanisms, each covering a failure mode the others can't:

| Failure mode               | What recovers it                                               |
| -------------------------- | -------------------------------------------------------------- |
| Health check fails cleanly | `OnFailure=` handler runs `silverblue-update --rollback` + reboot |
| Userspace hangs            | Hardware watchdog (`RuntimeWatchdogSec`) resets the machine     |
| New kernel won't load      | systemd-boot boot counting demotes the entry; previous root boots |

## How much disk space do snapshots cost?

Snapshots are Btrfs copy-on-write, so each one only costs the delta from its
neighbors. At most three are kept, pruned automatically. The ESP needs to be
≥ 512 MB, since systemd-boot requires per-snapshot kernels copied to it
(roughly 80–120 MB each).

## Can I install it on real hardware today?

Yes. The ISO ships `silverblue-install`, a **minimal plain-prompt installer**:
disk selection (type `ERASE` to confirm), hostname/timezone/locale/keymap,
bootloader choice, CPU microcode and `linux-firmware`, an optional network
stack (`systemd-networkd` or NetworkManager), a root password, and an optional
sudo-capable admin user. UEFI only, no disk encryption, no GUI — see
[Install on Real Hardware](/guides/installing/). The project is still young;
expect rough edges.

## Where do I download an ISO?

From [GitHub Releases](https://github.com/sinisterMage/Arch-silverblue/releases/latest)
(published on version tags, with `SHA256SUMS` for verification — run
`sha256sum -c SHA256SUMS`). ISOs are also built locally with `make build-iso`
or produced as CI artifacts on pushes to `main`.

## Which bootloaders are supported?

Both **systemd-boot** and **GRUB**, detected at runtime. systemd-boot is the
primary, CI-validated path. Because systemd-boot can't read Btrfs, kernels are
copied to the ESP per snapshot; because GRUB can't *write* Btrfs, its writable
`grubenv` lives on the ESP. No bootloader is patched — everything is stock
Arch packages.

## Does it work with ext4, LVM, or ZFS?

No — the mechanism is built on Btrfs subvolume snapshots. ZFS boot
environments map onto the same flow and are documented as
[future work](/architecture/update-flow/#zfs-future-work-not-implemented), but
not implemented.

## What about Secure Boot?

Out of scope for now, along with LUKS/disk encryption, swap setup, OTA/delta
updates, custom package signing, PXE, and any immutable-root enforcement.

## How is this different from snapper or Timeshift with grub-btrfs?

Those tools are **reactive**: they snapshot the live root, the update then
mutates the running system, and if something breaks *you* pick an old snapshot
from the boot menu and restore by hand. Arch Silverblue is **transactional**:
the update runs in a clone while your system stays untouched, the clone boots
exactly once, and it is promoted or reverted automatically based on a health
check — no manual intervention on failure.
