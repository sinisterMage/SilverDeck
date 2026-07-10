# SilverDeck

A gaming-console Linux distribution: boot straight into a controller-first,
fullscreen console UI — no desktop, no jargon. Atomic OS updates that roll
back by themselves if they break, a GUI installer that asks exactly two
things (which drive, are you sure), and a splash-masked boot from power-on to
UI.

**[Documentation](docs/index.md)** ·
[Install](docs/installation.md) ·
[User guide](docs/user-guide.md) ·
[Architecture](docs/architecture.md) ·
[Building](docs/building.md)

## How it fits together

- **`Arch-silverblue/`** — our fork of the
  [Arch Silverblue](https://github.com/sinisterMage/Arch-silverblue) atomic
  distro toolkit (Btrfs-snapshot updates with automatic rollback, the shared
  bash install engine, archiso image build, QEMU test harness). SilverDeck is
  a derivative per its fork-and-own model: `config/distro.conf` carries the
  SilverDeck identity, package set, and boot policy.
- **`silverdeck-ui/`** — the Rust workspace built on
  [GPUI](https://www.gpui.rs/) (pinned `=0.2.2`): the console shell
  (`silverdeck-ui`: Steam/Heroic/Flatpak library, curated Flathub store,
  settings) and the GUI installer (`silverdeck-installer`), sharing one
  palette, one input model (gamepad + keyboard), one code pattern.
- **`packaging/`** — pacman packages (`silverdeck-ui`, `-installer`,
  `-session`, `-plymouth`) built in an Arch container into a local repo the
  ISO bakes in.

Boot chain, installed: systemd-boot (menu hidden) → plymouth splash → greetd
autologin (`deck`) → sway kiosk → `silverdeck-ui`. The live ISO boots the
same kiosk into the installer instead. Details in
[docs/architecture.md](docs/architecture.md).

## Layout

| Path | Purpose |
|---|---|
| `silverdeck-ui/` | GPUI workspace: console shell, GUI installer, shared ui-kit, system crates |
| `packaging/` | PKGBUILDs + Arch builder container + local-repo build script |
| `Arch-silverblue/` | Distro toolkit fork (edited in place, additive only) |
| `docs/` | User + developer documentation |
| `.github/workflows/` | CI (checks, ISO) and release pipelines |

## Building

Development host expectation: Nix (dev shells) + Docker (Arch tooling). The
UI iterates on the host without touching the image:

```sh
make ui-check            # fmt + clippy + tests
make ui-run              # console shell in your Wayland session
make installer-run       # GUI installer against a fake engine (no root, no disks)
make ui-run-kiosk        # nested sway kiosk (fullscreen/IPC behavior)
```

Image pipeline (everything Arch-side runs in Docker):

```sh
make ui-package          # build the pacman packages -> local repo
make build-iso           # archiso build -> Arch-silverblue/iso/output/*.iso
make test-qemu           # install/update/rollback asserts in QEMU
make test                # ui-check + toolkit lint/bats/verify-units
```

More recipes (watching the installer in QEMU, logo regen, CI notes):
[docs/building.md](docs/building.md).

## Updates

OS updates are atomic (Btrfs snapshot + boot-count rollback), driven by
`silverdeck-update` from the UI's Settings tab. If a new snapshot can't bring
the console up, the bootloader falls back to the previous good one — no user
action. See [docs/updates-and-rollback.md](docs/updates-and-rollback.md).

## License

GPL-3.0 (see [LICENSE](LICENSE)).
