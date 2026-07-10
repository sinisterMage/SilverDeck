# Updates & rollback

SilverDeck updates are **atomic**: an update never modifies the system you are
running. Instead it:

1. Snapshots the current Btrfs root subvolume into a new `root-<timestamp>`.
2. Runs `pacman -Syu` inside the *new* snapshot.
3. Registers a boot entry for the new snapshot with **boot counting**
   (systemd-boot `+3` tries, or the GRUB `grubenv` equivalent).
4. On the next restart the machine boots the new snapshot.

After booting, a health check (`silverdeck-mark-good.service`) must succeed —
it requires the console UI to come up. When it does, the snapshot is marked
good. If the new snapshot fails to produce a working console within its boot
tries, the bootloader automatically falls back to the previous good snapshot
(`silverdeck-rollback`), and the machine is back where it was before the
update. No user action, no recovery USB.

At most 3 snapshots are retained (`KEEP_SNAPSHOTS` in
`Arch-silverblue/config/distro.conf`); older ones are pruned automatically.
`/home` lives on its own subvolume (`@home`) and is untouched by all of this.

Updates are started from **Settings → System update** in the UI (which starts
the root systemd unit `silverdeck-update.service` and streams its journal), or
from a shell with `sudo silverdeck-update`.

The complete flow — snapshot naming, ESP layout, boot-count mechanics, GRUB
`grubenv` scheme, pruning — is documented in the toolkit's
[`update-flow.md`](../Arch-silverblue/docs/update-flow.md).
