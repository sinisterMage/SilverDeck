# User guide

SilverDeck boots into a fullscreen console UI with three tabs. Everything is
reachable with a gamepad; a keyboard always works as well.

## Controls

| Action | Gamepad | Keyboard |
|---|---|---|
| Move | D-pad / left stick | Arrow keys |
| Select / confirm | A (south) | Enter |
| Back / cancel | B (east) | Escape |
| Next tab | RB | Tab |
| Previous tab | LB | Shift+Tab |
| Menu action (rescan, toggle gamescope, …) | Start | F1 |

Holding a direction auto-repeats (400 ms delay, then every 120 ms).

## Library

Your games from every source in one grid: Steam, Heroic (Epic/GOG), Flatpak
apps, and regular desktop entries. Selecting a game launches it fullscreen;
when it exits, focus returns to the UI. The Menu button toggles wrapping the
next launch in [gamescope](https://github.com/ValveSoftware/gamescope) (the
status bar shows the current setting). Menu also rescans the library.

## Store

A curated selection of games from Flathub. Confirm on an entry to install or
uninstall it (with a yes/no dialog); progress shows inline. Installed games
appear in the Library immediately.

## Settings

- **Wi-Fi** — toggle the radio, pick a network; secured networks open a
  controller-navigable on-screen keyboard (typing on a real keyboard works
  too).
- **Volume** — Left/Right adjusts the default output.
- **System update** — starts an atomic OS update and streams its log. Updates
  apply on the next restart and undo themselves automatically if the new
  version fails to boot — see [Updates & rollback](updates-and-rollback.md).
- **Restart / Power off** — with a confirmation dialog.

## Getting a Linux console

The UI is the whole user experience, but the system underneath is a normal
Arch-based Linux: switch to VT2 with `Ctrl+Alt+F2` and log in as `deck`. See
[Troubleshooting](troubleshooting.md) for details (including root access).
