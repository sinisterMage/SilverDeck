# SilverDeck documentation

SilverDeck is a gaming-console Linux distribution: it boots straight into a
controller-first, fullscreen console UI — no desktop, no jargon, atomic
updates with automatic rollback.

## For users

- [Installation](installation.md) — flash the ISO, follow the GUI installer.
- [User guide](user-guide.md) — the Library, Store, and Settings tabs; gamepad controls.
- [Updates & rollback](updates-and-rollback.md) — how updates apply and undo themselves.
- [Troubleshooting](troubleshooting.md) — consoles, boot menu, logs, FAQ.

## For developers

- [Architecture](architecture.md) — boot chain, snapshot model, repo layout, package flow.
- [Building](building.md) — dev shells, make targets, the ISO pipeline, QEMU recipes.
- [Installer internals](installer-internals.md) — the GUI ↔ bash-engine contract, markers, unattended mode.
- [Boot splash](boot-splash.md) — the plymouth theme and the masked-bootloader setup.
- [Contributing](contributing.md) — dev setup, test matrix, subtree policy.

## Upstream toolkit docs

SilverDeck derives from the Arch Silverblue atomic-distro toolkit, vendored at
`Arch-silverblue/` (fork-and-own). Its own docs remain authoritative for the
update engine and derivation model:

- [`Arch-silverblue/README.md`](../Arch-silverblue/README.md) — toolkit overview.
- [`Arch-silverblue/docs/update-flow.md`](../Arch-silverblue/docs/update-flow.md) — snapshot/rollback flow in detail.
- [`Arch-silverblue/docs/installing.md`](../Arch-silverblue/docs/installing.md) — the text installer.
- [`Arch-silverblue/DERIVING.md`](../Arch-silverblue/DERIVING.md) — the fork-and-own derivation guide.
