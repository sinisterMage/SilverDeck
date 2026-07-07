# Deriving your own distro from Arch Silverblue

Arch Silverblue is built so you can fork it into your own atomic Arch-based distro by editing
**one file** — [`config/distro.conf`](config/distro.conf) — and rebuilding. You do not need to
hunt through the scripts for the string `silverblue`; the build reads everything it needs to
rebrand from that config.

## How it works (fork-and-own model)

Your derivative is a **full copy of this repository** that you own and maintain. You edit
`config/distro.conf`, run the build, and ship your ISO. To pick up later upstream fixes you
`git merge` (or cherry-pick) from upstream like any other fork.

The build performs two strictly separated transformations, which is why a default
(unedited) build is identical to upstream:

1. **The source tree is never rewritten.** `src/` keeps its `silverblue` defaults so the unit
   tests keep working. Branding in the engine is just env-var indirection whose defaults equal
   today's values.
2. **Renaming/branding happens at build & install time.** [`iso/build.sh`](iso/build.sh) reads
   `config/distro.conf` and renders/renames *copies* of the tools into the ISO; the
   autoinstaller (which gets the same config copied into the ISO) lays those down on the target
   and writes `/etc/os-release`, hostname, timezone, locale, and any extra repos.

```
config/distro.conf ──► iso/build.sh ──► ISO (renamed binary, units, paths, os-release.in, config)
                              │
                              └──► autoinstaller (in the ISO) ──► installed target
                                     (os-release, hostname, tz, locale, repos, tools)
```

## Prerequisites

Same as upstream (see [README.md](README.md)):

- `make test` (lint + unit tests + unit verification): just needs `bash`; `shellcheck`/`bats`
  are pulled on demand via `nix shell`.
- `make build-iso`: Docker with `--privileged` and network (all Arch tooling runs in the container).
- `make test-qemu`: QEMU.

## The one file you edit: `config/distro.conf`

It is plain bash (`KEY="value"` and `arr=(...)`), sourced by the build. The defaults reproduce
stock Arch Silverblue. Variables, grouped:

### Identity / branding (also fills `/etc/os-release`)

| Variable | Purpose |
| --- | --- |
| `DISTRO_ID` | lowercase machine id → os-release `ID`, `LOGO`, and the Docker image tag |
| `DISTRO_NAME` | pretty name → boot-entry titles and os-release `NAME`/`PRETTY_NAME` |
| `DISTRO_VERSION` / `DISTRO_VERSION_ID` | os-release `BUILD_ID` / `VERSION_ID` |
| `DISTRO_ANSI_COLOR` | os-release `ANSI_COLOR` |
| `DISTRO_HOME_URL` | os-release `HOME_URL` **and** the `Documentation=` URL in the systemd units |
| `DISTRO_DOC_URL` / `DISTRO_SUPPORT_URL` / `DISTRO_BUG_URL` | the matching os-release URLs |

### Names & paths (rendered into the image only)

| Variable | Purpose | Required for a rename? |
| --- | --- | --- |
| `BIN_PREFIX` | the CLIs become `${BIN_PREFIX}-update` and `${BIN_PREFIX}-install` (`/usr/bin/silverblue-update` / `-install` by default) | yes |
| `UNIT_PREFIX` | the units become `${UNIT_PREFIX}-mark-good.service` / `-rollback.service` / `-rollback.target` | yes |
| `LIB_DIR` | helper/library dir (`/usr/lib/silverblue`) | yes |
| `ESP_SUBDIR` | per-snapshot kernel dir on the ESP (`/efi/silverblue/<snap>`, systemd-boot) | cosmetic |
| `SORT_KEY` | systemd-boot `sort-key` grouping value | cosmetic |
| `TOPLEVEL_MNT` | transient Btrfs `subvolid=5` mountpoint | cosmetic |

`BIN_PREFIX` and `UNIT_PREFIX` are usually set to the same value as `DISTRO_ID`.

### System defaults (written into the installed target)

| Variable | Purpose |
| --- | --- |
| `HOSTNAME` | `/etc/hostname` |
| `TIMEZONE` | zoneinfo path (e.g. `Europe/Amsterdam`) |
| `LOCALE` | `locale.gen` entry + `/etc/locale.conf` `LANG` |
| `KEYMAP` | vconsole keymap (`""` = leave default) |

These are what the unattended test install writes verbatim, and what the **interactive
installer** offers as prompt defaults (`BOOTLOADER` below likewise seeds its bootloader menu).

### Bootloader / filesystem

| Variable | Purpose |
| --- | --- |
| `BOOTLOADER` | default bootloader when none is forced (`systemd-boot` or `grub`) |
| `FS_LABEL` | Btrfs root label + GPT partition name |
| `ESP_LABEL` | FAT32 ESP label |
| `EFI_DIR` | ESP mountpoint on the installed system — **advanced; leave at `/efi`** (the autoinstaller's `grub-install`/mount paths assume `/efi`) |
| `KEEP_SNAPSHOTS` | max snapshots retained by the engine (`SB_KEEP`) |
| `BOOT_TRIES` | systemd-boot boot-counting tries (`SB_TRIES`) |

### Packages & repos

| Variable | Purpose |
| --- | --- |
| `PKGS_BASE` | array of packages `pacstrap`ped onto the target |
| `PKGS_ISO` | array of extra packages added to the live ISO so the installer can run |
| `EXTRA_REPOS` | array of complete `pacman.conf` section blocks appended to the target (networked installs only — see below) |

## What gets rendered where

| You set… | …and it lands in |
| --- | --- |
| `DISTRO_ID` | the ISO file name (`<id>-YYYY.MM.DD-x86_64.iso`) and the Docker image tag |
| `BIN_PREFIX` | `/usr/bin/<prefix>-update` and `/usr/bin/<prefix>-install`; the baked engine defaults; `file_permissions` in the ISO profile |
| `UNIT_PREFIX` | the three unit files + the two `*-mark-good.sh`/`*-rollback.sh` scripts; the enable symlink; the watchdog drop-in |
| `LIB_DIR` | where the engine finds its helpers/scripts (`SB_LIB_DIR` baked in) |
| `DISTRO_NAME` | systemd-boot/GRUB entry titles; unit `Description=`; os-release `NAME`/`PRETTY_NAME` |
| `DISTRO_*` URLs/ids | `/etc/os-release` (rendered from [`config/os-release.in`](config/os-release.in)) and unit `Documentation=` |
| `HOSTNAME` / `TIMEZONE` / `LOCALE` / `KEYMAP` | the installed `/etc/hostname`, `/etc/localtime`, `/etc/locale.conf`, `/etc/vconsole.conf` |
| `FS_LABEL` / `ESP_LABEL` | the Btrfs/ESP labels and the GPT partition name |
| `PKGS_ISO` | `packages.x86_64` in the archiso profile |
| `PKGS_BASE` | the `pacstrap` package list of both installers (the interactive one adds the user's microcode/firmware/network/sudo choices on top) |
| `EXTRA_REPOS` | the target's `/etc/pacman.conf` |

What deliberately stays generic: the whole `src/` tree on disk, the upstream unit tests,
`tools/verify-units.sh`, the uppercase `SILVERBLUE-*` progress markers (the QEMU harness greps
them), and the test-only `silverblue-autoinstall.sh` / synthetic `[silverblue-local]` repo.
The installer library and interactive frontend are installed **verbatim** (they read your
`distro.conf` at runtime); only the frontend's file *name* is derived from `BIN_PREFIX`.

## Branding

- Set `DISTRO_NAME` for the pretty name shown in the boot menu, `systemctl status`, and
  `/etc/os-release`.
- Tweak [`config/os-release.in`](config/os-release.in) directly if you want extra fields
  (e.g. a different `LOGO` or `ANSI_COLOR`). It's a template with `@TOKEN@` placeholders that
  the autoinstaller fills in from the config.

## Packages & repos

- Edit `PKGS_BASE` to change what's installed on the target (add `linux-firmware` for real
  hardware; add a desktop, NetworkManager, etc.).
- Edit `PKGS_ISO` if your installer needs more tools in the live environment.
- Add `EXTRA_REPOS` to point the installed system at your own pacman repo, e.g.:
  ```bash
  EXTRA_REPOS=(
    $'[mydistro]\nSigLevel = Optional TrustAll\nServer = https://repo.mydistro.org/$arch'
  )
  ```
  **Caveat:** `EXTRA_REPOS` is applied by the **interactive installer** (always networked) and
  by the unattended test install's networked path (`net=1`). The offline/hermetic test path
  leaves the target with just the bundled `file://` repo so the self-contained update test
  still works.

## Build & verify

```bash
# 1. Sanity-check the engine/units (independent of your config; must stay green):
make test

# 2. Build your ISO:
make build-iso        # -> iso/output/*.iso

# 3. (optional) Boot + install + update + rollback in QEMU:
make test-qemu
```

After `make build-iso`, confirm your rename took (replace `mydistro` with your `BIN_PREFIX`):

```bash
# Inspect the assembled airootfs staging (or extract the squashfs from the ISO):
grep -R "mydistro-update" iso/output/ 2>/dev/null   # or look in the build profile

# Inside the image you should find:
#   /usr/bin/mydistro-update
#   /usr/lib/mydistro/{sdboot-helpers.sh,grub-helpers.sh,mydistro-mark-good.sh,mydistro-rollback.sh}
#   /usr/lib/systemd/system/mydistro-mark-good.service  (ExecStart=/usr/lib/mydistro/...,
#                                                         OnFailure=mydistro-rollback.target)
#   the baked engine header: SB_DISTRO_NAME, SB_LIB_DIR, SB_VERIFY_UNIT set to your values
```

On the **installed** target, `cat /etc/os-release` shows your `ID`/`PRETTY_NAME`, `hostnamectl`
shows your hostname, the boot menu title shows your `DISTRO_NAME`, and `mydistro-update --dry-run`
runs.

## Constraints & gotchas

- **Keep the source defaults if you reuse the upstream unit tests.** The bats tests pin the
  *defaults* (`Arch Silverblue`, `sort-key silverblue`, ESP path `/silverblue/...`). They test
  the source tree, which is never rewritten, so they keep passing regardless of your config. If
  you rename and want a green `make test-qemu`, point `tests/qemu/harness.py` / `tests/qemu/run.sh`
  at your `BIN_PREFIX`-named binary and units (the harness uses the literal names).
- **sed-safe values.** `DISTRO_NAME` and the URLs are substituted with `sed`; the build escapes
  `&`, `|`, and `\`, but avoid embedding a literal `|` in `DISTRO_NAME` to be safe.
- **`ESP_SUBDIR` must agree everywhere.** Both the engine (baked) and the autoinstaller read it
  from this one config, so they stay in lockstep — don't hand-edit it in only one place.
- **`EFI_DIR`** is wired into the engine but the autoinstaller assumes `/efi`; leave it unless
  you're also willing to adjust the autoinstaller's `grub-install`/mount paths.

## FAQ / troubleshooting

- **`mark-good.service` fails to start / "permission denied".** The executables must be 0755 in
  the ISO. `iso/build.sh` injects `file_permissions` entries for your renamed paths; if you
  changed the injection, double-check those entries match `BIN_PREFIX`/`LIB_DIR`/`UNIT_PREFIX`.
- **`/etc/os-release` didn't change.** Arch ships `/etc/os-release` as a symlink to
  `/usr/lib/os-release`; the autoinstaller writes a *regular* file that overrides it. If you
  override `OSRELEASE_IN`, make sure the template still has the `@TOKEN@` placeholders.
- **Bootloader "not detected" after install.** Make sure `BOOTLOADER` is `systemd-boot` or
  `grub`, and that `ESP_SUBDIR` is consistent (see above).
- **Build uses the wrong image tag.** `IMAGE` in the `Makefile` is derived from `DISTRO_ID`; if
  the config can't be sourced it falls back to `arch-silverblue-iso`.
