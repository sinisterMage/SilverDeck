# Troubleshooting & FAQ

## Getting a console

- **On the installed system**: `Ctrl+Alt+F2` switches to a getty on VT2; log
  in as `deck`. `Ctrl+Alt+F1` returns to the UI.
- **On the live ISO**: same — VT2 has a getty (root, no password). The GUI
  installer runs on VT1.
- Serial consoles: if the system was installed over a serial console, its
  boot entries keep `console=ttyS0` and a getty stays reachable there.

## "The root account is locked — how do I administrate?"

The GUI install locks root on purpose (console-style device). Options:

- `deck` can run the things the UI needs via polkit (updates, power).
- For full root: boot the live USB, or use the text installer instead (it
  asks for a root password and an optional wheel/sudo admin user).
- From a `deck` shell on a GUI-installed system you can also
  `sudo silverdeck-update` *if* you created an admin user via the text
  installer; otherwise use the live USB to `passwd -u root` inside a chroot.

## Showing the boot menu

systemd-boot's menu is hidden (`timeout 0`). **Hold a key (Space works well)
right after power-on** to show it once. Every retained snapshot has an entry,
so you can boot an older snapshot manually from there. With GRUB, the menu
appears by itself (10 s) after a failed boot.

## An update broke something

Normally nothing to do: if the new snapshot can't bring up the console UI
within 3 boots, the bootloader falls back to the previous good snapshot
automatically. To roll back *manually*, show the boot menu (above) and pick
the previous `root-…` entry, then run `sudo silverdeck-update` again later.

## Install failed

The installer's *Show details* screen has the log tail; the full log is at
`/var/log/silverdeck-install.log` (on the live system — copy it off before
rebooting, e.g. `curl -F file=@/var/log/silverdeck-install.log 0x0.st` from
VT2). *Try again* re-runs from the drive picker; a half-written drive is fine
— the installer re-partitions from scratch.

Common causes:

- **No network / mirrors unreachable** — the engine needs the Arch mirrors;
  the installer's network screen retries. Corporate/captive networks are the
  usual culprit.
- **BIOS/CSM boot** — SilverDeck is UEFI-only; the preflight fails with
  "booted in BIOS mode". Enable UEFI in firmware settings.

## The screen stays black at boot

The splash hides everything by design. If it *stays* black past a minute:

- press `Esc` — plymouth switches to detail view (kernel/systemd messages);
- or switch to VT2 for a console;
- worst case, boot the previous snapshot from the boot menu.

## The UI crashed

The session wrapper restarts `silverdeck-ui` automatically with backoff.
Logs: `journalctl -t silverdeck-ui -t silverdeck-session` (or
`-t silverdeck-installer -t silverdeck-live-session` on the live ISO).

## Where is everything?

| Thing | Location |
|---|---|
| Install log (live) | `/var/log/silverdeck-install.log` |
| UI / session logs | `journalctl -t silverdeck-ui -t silverdeck-session` |
| Update log | `journalctl -u silverdeck-update` |
| Health check / rollback | `journalctl -u silverdeck-mark-good -u silverdeck-rollback` |
| Snapshots | `btrfs subvolume list /` (as root) |
| Distro configuration | `Arch-silverblue/config/distro.conf` (build time) |
