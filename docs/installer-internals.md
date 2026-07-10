# Installer internals

There is exactly **one install engine** — the bash library at
`Arch-silverblue/src/installer/install-lib.sh` — and three frontends that
drive it:

| Frontend | Where | How it answers questions |
|---|---|---|
| `silverdeck-installer` (GUI) | live ISO kiosk (VT1) | `SB_INST_*` environment (unattended mode) |
| `silverdeck-install` (text) | VT2 / serial | interactive prompts |
| `silverblue-autoinstall.sh` | QEMU test appliance | fw_cfg / env, hardcoded `/dev/vda` |

Partitioning (ESP + Btrfs), subvolumes, pacstrap, target configuration,
plymouth/initramfs, and both bootloader installs live in the library and are
exercised by the QEMU harness regardless of frontend.

## Marker protocol

With `SB_INSTALL_MARKERS=1` the engine emits machine-readable markers
(uppercase `SILVERBLUE-*` names are a compatibility contract with the QEMU
harness and are never renamed by the build):

```
SILVERBLUE-INSTALL-PROMPT key=<key>      # stderr, before each interactive prompt
SILVERBLUE-INSTALL-START disk=… snap=… bootloader=…
SILVERBLUE-INSTALL-STEP name=partition|filesystems|pacstrap|configure|bootloader
SILVERBLUE-INSTALL-OK snap=… uuid=… bootloader=…
SILVERBLUE-INSTALL-FAIL line=<n>
```

The **prompt order of the interactive frontend is a contract** with
`tests/qemu/harness.py` — never reorder it.

## Unattended mode (what the GUI uses)

```sh
SB_INSTALL_MARKERS=1 SB_INST_UNATTENDED=1 \
SB_INST_DISK=/dev/nvme0n1 SB_INST_CONFIRM=ERASE \
silverdeck-install
```

- `SB_INST_DISK` (required) must be one of the disks printed by
  `silverdeck-install --list-disks` (`PATH|SIZE|MODEL` lines, live medium
  excluded — the GUI's disk picker is fed by this).
- `SB_INST_CONFIRM=ERASE` (required) is the defense-in-depth erase gate.
- Everything else defaults from `config/distro.conf`:
  hostname/timezone/locale/keymap/bootloader, auto-detected microcode,
  `linux-firmware` yes, NetworkManager, and — because
  `INSTALL_LOCK_ROOT=1` — a locked root account (`SB_INST_LOCK_ROOT=0` +
  `SB_INST_ROOT_PASSWORD=…` overrides). The `deck` user is not created by the
  installer at all: it comes from `silverdeck-session`'s sysusers file at
  pacstrap time.
- Overridables: `SB_INST_HOSTNAME`, `SB_INST_TIMEZONE`, `SB_INST_LOCALE`,
  `SB_INST_KEYMAP`, `SB_INST_BOOTLOADER` (`systemd-boot`|`grub`),
  `SB_INST_MICROCODE`, `SB_INST_FIRMWARE`, `SB_INST_NETWORK`
  (`none`|`networkd`|`networkmanager`), `SB_INST_USERNAME`/`SB_INST_USER_PASSWORD`.

The engine appends `KERNEL_OPTS_EXTRA` (distro.conf: `quiet splash …`) plus
any serial `console=` options to the boot entry; the update engine carries
those into every later snapshot's entry.

## The GUI side

`silverdeck-ui/crates/installer/src/engine.rs` is the only process-facing
module:

- spawns the command above with stdout+stderr piped, tees every line to
  `/var/log/silverdeck-install.log`;
- maps `STEP` markers to stages/percentages (pacstrap owns 15→85% and creeps
  per output line), non-marker lines to the "last line" readout;
- treats *exit 0 + the OK marker* as success — anything else lands on the
  Failed screen with the log tail.

`SILVERDECK_FAKE_INSTALL=1` (see [Building](building.md)) swaps in a canned
event stream so the whole UI is demoable on a dev host; `=fail` exercises the
failure path. `SILVERDECK_INSTALL_CMD` overrides the engine binary name.

## QEMU harness modes

- `make test-qemu` — unattended install (autoinstaller) + update + rollback
  asserts, driven over the serial console.
- `make -C Arch-silverblue test-qemu-interactive` — types answers into the
  *text* installer keyed off the PROMPT markers, then boots the result.

Both must stay green after any engine change; the autoinstaller path also
exercises `configure_plymouth` + `KERNEL_OPTS_EXTRA`, proving quiet/splash
doesn't break the serial contract.
