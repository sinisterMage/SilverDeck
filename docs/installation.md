# Installing SilverDeck

## What you need

- A PC with UEFI firmware (BIOS-only machines are not supported).
- A USB stick (2 GB or more — it will be erased).
- A wired network connection or Wi-Fi.
- A gamepad or a keyboard (both work everywhere in the installer).

## 1. Flash the ISO

Download the latest ISO and `SHA256SUMS` from the
[releases page](https://github.com/sinisterMage/SilverDeck/releases), verify,
and write it to the USB stick:

```sh
sha256sum -c SHA256SUMS
# Linux/macOS (replace sdX with the USB device — double-check!):
sudo dd if=silverdeck-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
```

On Windows, [Rufus](https://rufus.ie/) or
[balenaEtcher](https://etcher.balena.io/) work fine.

## 2. Boot the USB stick

Boot the machine from the stick (usually a boot-device key like F12/F8/Esc at
power-on). The live system starts straight into the SilverDeck installer.

## 3. Follow the installer

The flow is deliberately short:

1. **Welcome** — pick *Install SilverDeck*.
2. **Connect to the internet** — only appears if you're offline. Pick a Wi-Fi
   network (the on-screen keyboard takes the password; a physical keyboard
   works too), or plug in a cable and choose *Check again*.
3. **Choose where to install** — drives are listed by size and model. The USB
   stick you booted from is never shown.
4. **Confirm** — the one scary screen: *everything on the chosen drive is
   erased*. "Go back" is the default; you have to deliberately move to
   *Erase and install*.
5. **Progress** — a few minutes while SilverDeck downloads and installs.
6. **All set** — press *Restart* and unplug the stick when the screen goes
   dark.

Everything else is preconfigured: the `deck` user is created automatically and
the console boots straight into the SilverDeck UI. There are no passwords to
invent during install (the root account is locked; see the
[FAQ](troubleshooting.md) for how to get a shell).

## Text installer (fallback / power users)

The plain-prompt text installer is still there and asks the full set of
questions (hostname, timezone, locale, bootloader, root password, admin user):

- Switch to the console on **VT2** (`Ctrl+Alt+F2`, log in as `root`, no
  password) or use the serial console, then run:

```sh
silverdeck-install
```

See [`Arch-silverblue/docs/installing.md`](../Arch-silverblue/docs/installing.md)
for the full walkthrough of the text flow.

## After the first boot

The first boot sets up the Flathub app store in the background. The system
verifies itself on every boot and rolls back automatically if an update ever
breaks it — see [Updates & rollback](updates-and-rollback.md).
