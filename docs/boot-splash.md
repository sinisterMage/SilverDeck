# Boot splash & masked bootloader

Goal: power-on → dark screen with the SilverDeck spark → the console UI.
No bootloader menu, no systemd wall of text — unless something goes wrong.

## The pieces

| Piece | Where | What it does |
|---|---|---|
| `silverdeck-plymouth` package | `packaging/silverdeck-plymouth/` | plymouth *script* theme: `#0b0f14` background, the spark + wordmark, a `#38bdf8` pulse bar (UI palette) |
| `PLYMOUTH_THEME="silverdeck"` | `config/distro.conf` | theme the installer selects |
| `configure_plymouth` | `install-lib.sh` | inserts the `plymouth` mkinitcpio hook after `udev` and runs `plymouth-set-default-theme`; no-op when plymouth isn't on the target |
| `KERNEL_OPTS_EXTRA` | `config/distro.conf` | `quiet splash loglevel=3 rd.udev.log_level=3 systemd.show_status=auto vt.global_cursor_default=0` on every boot entry |
| `SDBOOT_TIMEOUT="0"` / `GRUB_TIMEOUT="0"` | `config/distro.conf` | hide the bootloader menu |
| greetd drop-in `plymouth.conf` | `packaging/silverdeck-session/` | `After=/Wants=plymouth-quit-wait.service` — plymouth exits cleanly before greetd takes the VT |

The update engine preserves the kernel options of the running system when it
registers new snapshots, so splash survives every update automatically.

## "…unless something fails"

- **systemd-boot** (default): `timeout 0` hides the menu; **hold a key (e.g.
  Space) while the firmware hands off** to bring it up on demand. Boot
  counting still demotes a snapshot that fails its health check, and the
  automatic rollback needs no menu at all.
- **GRUB**: `timeout 0` normally; after a failed boot the `recordfail` flag
  forces the menu for 10 seconds (that logic ships in the generated
  `grub.cfg`).
- A boot that degrades far enough to matter also stops being quiet on its
  own: `systemd.show_status=auto` prints status when units fail, and the
  rollback path drops to the emergency target if there is nothing to roll
  back to.

## Scope

- The **live ISO is not splashed** — it uses archiso's stock (verbose) boot.
  The live environment is a serial-driven test surface and an installer
  kiosk; splash is an installed-system feature.
- No LUKS on SilverDeck, so the theme's password prompt path is defensive
  only.

## Editing the theme

Theme files install to `/usr/share/plymouth/themes/silverdeck/`. Sources:

- `packaging/silverdeck-plymouth/files/silverdeck.script` — layout, pulse
  animation, message/prompt handlers (plymouth *script* plugin).
- `silverdeck-ui/assets/logo.svg` — logo source; regenerate the committed PNG
  with the `rsvg-convert` command in [Building](building.md).

Preview on any Arch box with plymouth installed:

```sh
sudo plymouth-set-default-theme silverdeck
sudo plymouthd --debug --no-daemon &   # in one terminal
sudo plymouth show-splash              # in another; quit with: plymouth quit
```

The real check is the QEMU flow: `make test-qemu` installs a target that
boots with the full quiet/splash/plymouth stack, and the harness's health
asserts prove it still reaches the UI.
