# SilverDeck

A gaming-console Linux distribution: boot straight into a controller-first,
fullscreen console UI — no desktop.

SilverDeck is built from two parts:

- **`Arch-silverblue/`** — our fork of the
  [Arch Silverblue](https://github.com/sinisterMage/Arch-silverblue) atomic
  distro toolkit (Btrfs-snapshot updates with automatic rollback, archiso
  image build, QEMU test harness). SilverDeck is a derivative per its
  fork-and-own model: `config/distro.conf` carries the SilverDeck identity and
  package set.
- **`silverdeck-ui/`** — the console UI, a Rust workspace built on
  [GPUI](https://www.gpui.rs/) (pinned `=0.2.2`). Launcher-agnostic game
  library (Steam, Heroic/Epic/GOG, Flatpak, desktop entries), a curated
  Flathub game store, and settings (Wi-Fi, audio, atomic OS updates, power) —
  all fully navigable with a gamepad.

The installed system boots: systemd → greetd autologin (user `deck`) → sway
(kiosk config) → `silverdeck-ui` fullscreen. Games launch as sway clients,
optionally wrapped in gamescope; when a game exits, focus returns to the UI.

## Layout

| Path | Purpose |
|---|---|
| `silverdeck-ui/` | GPUI console UI workspace (see its crates/ tree) |
| `packaging/` | PKGBUILDs + Arch builder container + local-repo build script |
| `packaging/silverdeck-session/` | Kiosk session package: greetd/sway config, udev controller rules, polkit rules, firstboot + update units, store allowlist |
| `Arch-silverblue/` | Distro toolkit fork (edited in place, additive only) |

## Building

Development host expectation: Nix (dev shells) + Docker (Arch tooling). The
UI iterates on the host without touching the image:

```sh
make ui-check          # clippy + tests
make ui-run            # run the UI in your Wayland session
make ui-run-kiosk      # nested sway kiosk (fullscreen/IPC behavior)
make ui-run-lavapipe   # software-Vulkan path (what QEMU uses)
```

Image pipeline (everything Arch-side runs in Docker):

```sh
make ui-package        # build silverdeck-ui/-session pacman pkgs -> local repo
make build-iso         # archiso build -> Arch-silverblue/iso/output/*.iso
make test-qemu         # install/update/rollback + kiosk session asserts
make test              # upstream lint+bats + ui-check
```

## Updates

OS updates are atomic (Btrfs snapshot + boot-count rollback), driven by
`silverdeck-update` — triggered from the UI's Settings tab via a root
systemd unit. The UI/session packages ship in a local pacman repo baked into
the image at `/var/lib/silverdeck/repo`; point the `[silverdeck]` repo at an
https server later to ship UI updates out-of-band.
