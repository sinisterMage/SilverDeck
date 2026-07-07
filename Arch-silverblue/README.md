# Arch Silverblue

A **mutable** Arch Linux distribution with **atomic, transactional, auto-rolling-back** system
updates. Updates build a brand-new root in a Btrfs snapshot, boot it once, and only keep it if
the boot is healthy — otherwise the system reverts to the previous root. Unlike Fedora
Silverblue, the running system stays **fully writable**: no read-only `/usr`, no overlayfs, no
immutable root. Atomicity comes purely from Btrfs copy-on-write snapshots.

```
silverblue-update            # snapshot the root, upgrade it, validate, boot it next
                             #   → reboot → auto-marked good, or auto-rolled-back on failure
silverblue-update --rollback # boot the previous snapshot on next reboot
silverblue-update --dry-run  # show the plan without changing anything
```

See [`docs/update-flow.md`](docs/update-flow.md) for the full update/rollback flow with a
diagram. To fork this into your own branded distro, see [`DERIVING.md`](DERIVING.md) — you edit
one file ([`config/distro.conf`](config/distro.conf)) and rebuild.

## How it works (in one paragraph)

Each update takes a copy-on-write `btrfs subvolume snapshot` of the running root into
`root-YYYYMMDD-HHMMSS`, `arch-chroot`s in and runs `pacman -Syu`, validates the result
(kernel + initramfs present, `systemd-analyze verify` clean), registers it as a new bootloader
entry, and makes it the **next** boot without changing the **permanent default**. After reboot,
`silverblue-mark-good.service` health-checks the boot: on success the new root becomes the
permanent default; on failure/timeout/hang the bootloader falls back to the previous root via
boot counting + a watchdog. At most three snapshots are kept.

Both **systemd-boot** and **GRUB** are supported and detected at runtime. `/boot` lives inside
each snapshot; for systemd-boot (which can't read Btrfs) the per-snapshot kernel is copied to
the ESP, and for GRUB (which can't *write* Btrfs) the writable `grubenv` lives on the ESP.

## Repository layout

```
src/
  update-engine/silverblue-update      # the atomic update CLI (Bash, shellcheck-clean)
  bootloader/sdboot-helpers.sh         # systemd-boot entry/copy/bless/prune helpers
  bootloader/grub-helpers.sh           # GRUB menuentry/grubenv helpers
  installer/install-lib.sh             # shared install steps (partition/pacstrap/bootloader)
  installer/silverblue-install         # minimal interactive installer (runs on the live ISO)
  init/silverblue-mark-good.{service,sh}   # post-boot health check + good-marking
  init/silverblue-rollback.{target,service,sh}  # OnFailure rollback
  init/silverblue-watchdog.conf        # RuntimeWatchdogSec drop-in (hang recovery)
iso/
  Dockerfile                           # reproducible archiso build image
  build.sh                             # assembles the releng profile + overlay, runs mkarchiso
  airootfs/                            # ISO overlay: test autoinstaller, serial autologin, fw_cfg
tests/
  unit/                                # bats tests + command mocks (no root/Btrfs needed)
  qemu/run.sh, qemu/harness.py         # boot the ISO in QEMU; install/update/rollback tests
docs/update-flow.md                    # documented flow + ASCII diagram
docs/installing.md                     # end-user install guide (real hardware)
Makefile                               # lint / test-unit / build-iso / test-qemu
```

## Prerequisites

The **dev host needs only `docker` and `qemu-system-x86_64`**. All Arch tooling runs inside the
build container and the QEMU guest. `shellcheck` and `bats` are fetched on demand via
`nix shell` (the Makefile does this for you); if you are not on Nix, install `shellcheck` and
`bats-core` yourself and run them directly.

| Task                | Needs                                                      |
|---------------------|-----------------------------------------------------------|
| `make lint`         | `nix` (or host `shellcheck`)                               |
| `make test-unit`    | `nix` (or host `bats-core`) + `bash`                       |
| `make build-iso`    | `docker` with `--privileged` (mkarchiso needs loop/mount); network to fetch packages |
| `make test-qemu`    | `qemu-system-x86_64`, OVMF firmware, `python3`; KVM optional (TCG fallback) |

## Quick start

```bash
# Fast inner loop (no docker/qemu, no root, no Btrfs):
make lint          # shellcheck every script — zero findings
make test-unit     # bats unit tests for naming / pruning / detection / rendering

# Full pipeline:
make build-iso     # produces iso/output/*.iso  (privileged docker, downloads packages)
make test-qemu     # boots the ISO, installs, runs the happy-path + rollback assertions
```

`make test-qemu` exits 0 only if both the happy-path (update → reboot → marked good) and the
rollback path (bad update → reverts to previous root) pass. It uses KVM when `/dev/kvm` is
writable and falls back to TCG (`-cpu qemu64`) otherwise.

## Installing on real hardware

Download the ISO from
[GitHub Releases](https://github.com/sinisterMage/Arch-silverblue/releases/latest) (published
automatically on version tags), boot it in UEFI mode, and run **`silverblue-install`** — a
minimal plain-prompt installer that asks for disk, hostname, timezone/locale/keymap,
bootloader, CPU microcode, `linux-firmware`, a network stack, a root password, and an optional
admin user, then requires typing `ERASE` before touching anything. See
[`docs/installing.md`](docs/installing.md) for the full guide.

## Usage on an installed system

`silverblue-update` must run as root on a Btrfs root with an ESP at `/efi`.

```
silverblue-update [--dry-run] [--verbose] [--rollback]

  (no args)    Snapshot the running root, upgrade it, validate it, and register it as the
               next boot target without changing the permanent default.
  --rollback   Set the previous snapshot as the next boot target (manual rollback).
  --dry-run    Print the planned steps without making any changes.
  --verbose    Echo each privileged command as it runs.
```

Key environment overrides (see the top of `src/update-engine/silverblue-update`): `SB_EFI_DIR`
(default `/efi`), `SB_KEEP` (default `3`), `SB_TRIES` (systemd-boot boot-counting tries,
default `3`), and the injectable command paths `BTRFS`, `BOOTCTL`, `GRUB_EDITENV`,
`ARCH_CHROOT` (used by the unit tests to mock externals).

## Testing details

- **Unit tests** (`tests/unit/`, bats) source the engine with mocked command paths and exercise
  the pure logic — snapshot naming, prune planning, bootloader detection, and entry rendering —
  with no root, Btrfs, or QEMU required.
- **Integration test** (`tests/qemu/`) boots the built ISO headless over a serial console. An
  autoinstaller (gated by a QEMU `fw_cfg` blob, so it never fires on a normal boot) lays down
  Arch Silverblue on a virtual disk; `harness.py` then drives the installed system to assert the
  happy path and the rollback path. The **update cycle is hermetic by default** (a synthetic
  bumped package in an offline `file://` repo baked into the ISO); pass `--net` to instead do a
  real `pacman -Syu` over QEMU user-net. The integration test is validated against systemd-boot.
  > The install phase pacstraps a base system, which requires network (QEMU user-net) — this is
  > intrinsic to installing Arch. The *update* assertion is what runs offline.
- **Interactive-installer test** (`make test-qemu-interactive`) drives `silverblue-install`
  itself over the serial console: the harness answers every prompt via the
  `SILVERBLUE-INSTALL-PROMPT` markers, confirms with `ERASE`, then boots the installed system,
  logs in with the password it set, and asserts the subvolume, hostname, mark-good, network
  stack, admin user, and that no test-only artifacts (autologin, local test repo) leaked onto
  the target. The interactive installer and the test autoinstaller share one implementation
  (`src/installer/install-lib.sh`), so the unattended scenarios also cover the shared steps.

## Verification checklist

```bash
nix shell nixpkgs#shellcheck --command shellcheck src/update-engine/silverblue-update   # 0 findings
nix shell nixpkgs#shellcheck --command shellcheck tests/qemu/run.sh                      # 0 findings
make build-iso                                                                            # → iso/output/*.iso
make test-qemu                                                                            # happy + rollback PASS
systemd-analyze verify iso/airootfs/usr/lib/systemd/system/silverblue-mark-good.service   # (after build-iso stages it)
silverblue-update --dry-run --verbose                                                     # prints plan, no changes
silverblue-update --rollback                                                              # next boot = previous snapshot
```

## Design notes & limitations

- **systemd-boot is the primary, CI-validated path.** GRUB is fully implemented, covered by unit
  tests, and the QEMU harness can drive it end-to-end (`tests/qemu/run.sh --bootloader grub`).
  One GRUB limitation: a kernel that fails to *load* is not recovered unattended (stock GRUB has
  no in-session fallback; the held menu offers the previous root one keypress away) — the GRUB
  rollback test therefore exercises the health-check `OnFailure` path, while the systemd-boot
  test exercises boot counting with a corrupt kernel.
- **GRUB cannot write Btrfs**, so `grubenv` is kept on the FAT ESP (`/efi/grub/grubenv`). This
  avoids patching GRUB and keeps everything within stock Arch packages.
- **Boot counting cannot reboot a hang by itself** — `RuntimeWatchdogSec` (a hardware watchdog)
  plus the rollback `OnFailure` handler provide that. The QEMU harness does **not** attach an
  emulated watchdog: systemd cannot cleanly stop QEMU's `i6300esb` on reboot, so it stays armed
  and reboot-loops the VM. The hang-recovery path is therefore a documented mechanism that the
  automated test does not exercise; the rollback test instead drives boot counting directly.
- **ESP sizing:** keep ≤3 snapshots × ~80–120 MB of kernels ⇒ ESP ≥ 512 MB. Pruning deletes ESP
  kernels in lockstep with the subvolume.

### Out of scope

Any GUI (a minimal plain-prompt TUI installer *is* included — see
[`docs/installing.md`](docs/installing.md)), LUKS/disk encryption, swap setup, ZFS
implementation (documented as future work in `docs/update-flow.md`), OTA/delta updates, custom
pacman wrappers or signing, PXE, Secure Boot, and any immutable-root enforcement.

## Contributing

Run `make test` (lint + unit tests) before sending changes — it is fast and needs no
docker/qemu. Keep all shell scripts `shellcheck`-clean (`shellcheck -x`), and prefer adding to
the pure, unit-testable functions in `silverblue-update` so logic can be tested without root or
Btrfs. The engine is intentionally **sourceable** (it only runs `main` when executed directly),
and its state-changing wrapper is `sb_run` so it can be sourced into bats without clobbering
bats's own `run`.
