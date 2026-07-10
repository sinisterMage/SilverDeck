# Architecture

## Boot chain (installed system)

```
UEFI firmware
  └─ systemd-boot (menu hidden: timeout 0 — hold a key to show it)
       └─ linux + initramfs   (quiet splash; plymouth from early KMS)
            └─ systemd
                 ├─ plymouth splash (spark + pulse, palette-matched)
                 ├─ NetworkManager
                 ├─ silverdeck-mark-good.service  (health check → rollback on failure)
                 └─ greetd (after plymouth quits)
                      └─ autologin: user 'deck' → sway (kiosk config)
                           └─ silverdeck-ui (fullscreen, GPUI)
                                └─ games as sway clients (optionally gamescope)
```

The live ISO boots a parallel kiosk into the **installer** instead:
`greetd (root) → sway (live config) → silverdeck-installer`, with a getty on
VT2 and the serial console untouched (the QEMU harness depends on it). The
live boot is not splashed — splash is an installed-system feature.

## Storage & update model

Single Btrfs pool, no LUKS, no swap partition:

- `root-<timestamp>` subvolumes — one per OS snapshot; the kernel cmdline's
  `rootflags=subvol=` picks the booted one. Boot-counting demotes entries that
  fail their health check and the bootloader falls back to the previous good
  snapshot.
- `@home` — user data, shared by all snapshots.
- ESP at `/efi`; with systemd-boot each snapshot's kernel/initramfs lives at
  `/efi/silverdeck/<snap>/`.

See [Updates & rollback](updates-and-rollback.md) and the toolkit's
[`update-flow.md`](../Arch-silverblue/docs/update-flow.md).

## Repository layout

| Path | What |
|---|---|
| `silverdeck-ui/` | Rust workspace (GPUI `=0.2.2`, pinned) |
| `silverdeck-ui/crates/app` | `silverdeck-ui` — the console shell (Library/Store/Settings) |
| `silverdeck-ui/crates/installer` | `silverdeck-installer` — the GUI installer |
| `silverdeck-ui/crates/ui-kit` | shared palette + on-screen keyboard |
| `silverdeck-ui/crates/{core,input,launch,sources,store,system}` | domain model, gamepad, session lifecycle, game discovery, Flathub, system control |
| `packaging/` | PKGBUILDs (`silverdeck-ui`, `-installer`, `-session`, `-plymouth`) + builder container + `build-repo.sh` |
| `Arch-silverblue/` | distro toolkit fork: install engine, update engine, archiso build, QEMU harness (edited in place, additive only) |
| `Arch-silverblue/config/distro.conf` | the single source of truth: identity, package sets, boot options, timeouts |
| `.github/workflows/` | CI + release pipelines |

## UI application pattern

Both GPUI apps follow the same shape: a single root view owns all state; each
tab/screen is a module exporting `struct State`, `handle_nav(root, NavEvent,
cx)`, and `render(root, cx)`. Gamepad events (a `gilrs` thread →
`async_channel`) and keyboard actions converge on the same `handle_nav`.
Blocking work runs on the background executor and updates state via
`this.update(cx, …)` + `cx.notify()`. Shell commands go through the
`CommandRunner` trait (`silverdeck-system`) so tests can fake them.

## Package flow

```
packaging/*/PKGBUILD ──build-repo.sh (Arch container)──▶ Arch-silverblue/iso/local-repo/
                                                              │  baked into the ISO at /opt/silverdeck/repo
                                                              ▼
              live ISO: pacman.conf [silverdeck] entry ──▶ pacstrap resolves silverdeck-* packages
                                                              │  installer copies the repo to the target
                                                              ▼
              installed system: /var/lib/silverdeck/repo ──▶ survives snapshots; updates keep resolving
```

Swap the `[silverdeck]` `Server=` for an https URL (in `EXTRA_REPOS`,
`config/distro.conf`) to ship UI updates out-of-band.
