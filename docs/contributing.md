# Contributing

## Dev setup

- **Nix** with flakes (the UI dev shell provides rust, clippy, rustfmt, and
  every C library GPUI dlopens — no system Rust needed).
- **Docker** (all Arch tooling — makepkg, mkarchiso — runs in containers).
- **QEMU + OVMF** for the integration tests.

First loop: `make ui-check && make installer-run`.

## Test matrix (what must stay green)

| Command | Covers | Cost |
|---|---|---|
| `make ui-check` | rustfmt, clippy `-D warnings`, all Rust tests | ~min |
| `make -C Arch-silverblue test` | shellcheck, bats units (incl. the installer env contract), systemd unit verify | ~min |
| `make build-iso` | packages + full ISO assembly | ~30-60 min, network |
| `make test-qemu` | unattended install → boot → update → rollback (serial-driven) | long |
| `make -C Arch-silverblue test-qemu-interactive` | the text installer's prompt contract | long |

CI runs the first two on every push/PR; the ISO on default-branch pushes; the
QEMU jobs on manual dispatch (see [Building](building.md)).

## Contracts to respect

- **Interactive prompt order** in `silverblue-install` and the
  `SILVERBLUE-*` marker names — the QEMU harness depends on both. Extend the
  unattended env path instead of touching prompts.
- **GPUI is pinned `=0.2.2`** — bump deliberately, never implicitly.
- **`config/distro.conf` is data, not code** — plain assignments only; both
  build host and live ISO source it.
- New engine behavior needs a bats test in `Arch-silverblue/tests/unit/`
  (mocks for `arch-chroot`/`bootctl`/… are in `tests/unit/mocks/`).

## Subtree policy

`Arch-silverblue/` is a fork-and-own copy of the upstream toolkit, **edited in
place, additively**: prefer new functions/keys with upstream-preserving
defaults (`${SDBOOT_TIMEOUT:-3}`-style) so diffs against upstream stay
reviewable and re-syncs stay possible. It keeps its own
[CONTRIBUTING.md](../Arch-silverblue/CONTRIBUTING.md), README and docs.

## UI code style

Follow the existing module pattern (`State` + `handle_nav` + `render` per
screen/tab; async via `cx.spawn` + background executor; shelling out via
`CommandRunner`). `cargo fmt` is enforced by `make ui-check`. User-facing
installer text stays jargon-free — "drive", not "block device".
