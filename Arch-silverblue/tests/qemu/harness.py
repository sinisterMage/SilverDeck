#!/usr/bin/env python3
"""Serial-console driver for the Arch Silverblue QEMU integration test.

Invoked by run.sh (which exports the SB_* environment below). It runs three QEMU phases
against one persistent virtual disk:

  1. install  — boot the ISO; the fw_cfg-gated autoinstaller lays down Arch Silverblue and
                powers off. We wait for SILVERBLUE-INSTALL-OK.
  2. happy    — boot the disk; assert the first boot marked the root good; run one update
                cycle (hermetic by default); reboot; assert the new root booted and was
                marked good.
  3. rollback — boot the disk; run an update with a single boot-counting try, corrupt the
                new root's kernel (on the ESP for systemd-boot, inside the subvolume for GRUB),
                reboot; assert the system fell back to the previous (good) root.

With SB_INTERACTIVE=1 (run.sh --interactive) it instead drives the interactive installer
over the serial console — answering its prompts via the SILVERBLUE-INSTALL-PROMPT markers
(SB_INSTALL_MARKERS=1; the prompt ORDER is a contract with gather_answers() in
src/installer/silverblue-install) — then boots the installed system, logs in with the
password it set, and verifies the result:

  1. interactive-install — boot the ISO, run silverblue-install, answer every prompt,
                confirm with ERASE, wait for SILVERBLUE-INSTALL-OK, decline the reboot.
  2. interactive-boot    — boot the disk, log in as root at the serial getty (no autologin
                on interactive targets), assert the right subvol/hostname/mark-good, the
                enabled network stack, the admin user + sudoers drop-in, and that no
                test-only artifacts (autologin drop-in, /opt/silverblue) were installed.

Each scenario prints PASS/FAIL; the process exits 0 only if all pass (CI-friendly).
Only the Python standard library is used.
"""

import os
import re
import sys
import threading
import time

ISO = os.environ["SB_ISO"]
DISK = os.environ["SB_DISK"]
FW_CODE = os.environ["SB_FW_CODE"]
FW_VARS = os.environ["SB_FW_VARS"]
ACCEL = os.environ.get("SB_ACCEL", "tcg")
CPU = os.environ.get("SB_CPU", "qemu64")
NET = os.environ.get("SB_NET", "0")
BOOTLOADER = os.environ.get("SB_BOOTLOADER", "systemd-boot")
INTERACTIVE = os.environ.get("SB_INTERACTIVE", "0") == "1"
WORK = os.environ.get("SB_WORK", ".")

# Derivative identity (exported by run.sh from config/distro.conf). The SILVERBLUE-*
# progress markers stay literal by design; only the tool/unit NAMES are derived.
BIN_PREFIX = os.environ.get("SB_BIN_PREFIX", "silverblue")
UNIT_PREFIX = os.environ.get("SB_UNIT_PREFIX", "silverblue")
ESP_SUBDIR = os.environ.get("SB_ESP_SUBDIR", "silverblue")
# Assert the kiosk session (greetd -> sway -> <bin_prefix>-ui) on the installed target.
SESSION = os.environ.get("SB_SESSION", "0") == "1"
UI_READY_MARKER = "%s-UI-READY" % BIN_PREFIX.upper()

# Credentials the interactive scenario feeds the installer (test-only values).
ROOT_PW = "sbtest-root-pw"
USER_PW = "sbtest-user-pw"

# TCG is much slower than KVM, so scale timeouts accordingly.
SLOW = ACCEL != "kvm"
T_INSTALL = 3600 if SLOW else 1200
T_BOOT = 600 if SLOW else 300
T_CMD = 600 if SLOW else 180
T_UPDATE = 1200 if SLOW else 300


class ConsoleError(Exception):
    pass


class Console:
    """Drives one QEMU instance over its serial stdio with an expect()/send() API."""

    def __init__(self, name, extra_args):
        import subprocess

        self.name = name
        self.buf = ""
        self.pos = 0
        self.lock = threading.Lock()
        self.logpath = os.path.join(WORK, "serial-%s.log" % name)
        self.logf = open(self.logpath, "w", encoding="utf-8", errors="replace")
        args = self._base_args() + extra_args
        self.logf.write("# qemu: %s\n" % " ".join(args))
        self.logf.flush()
        self.proc = subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, bufsize=0,
        )
        self.reader = threading.Thread(target=self._read_loop, daemon=True)
        self.reader.start()

    def _base_args(self):
        # Kiosk targets get a KMS-capable display device (sway needs /dev/dri even with
        # the pixman software renderer) and more RAM (compositor + UI on llvmpipe).
        gpu = ["-device", "virtio-gpu-pci"] if SESSION else []
        return [
            "qemu-system-x86_64",
            "-machine", "q35,accel=%s" % ACCEL,
            "-cpu", CPU,
            "-m", "3072" if SESSION else "2048", "-smp", "2",
        ] + gpu + [
            "-drive", "if=pflash,format=raw,readonly=on,file=%s" % FW_CODE,
            "-drive", "if=pflash,format=raw,file=%s" % FW_VARS,
            "-drive", "file=%s,if=virtio,format=qcow2" % DISK,
            "-netdev", "user,id=net0",
            "-device", "virtio-net-pci,netdev=net0",
            # NOTE: no i6300esb watchdog. systemd cannot stop the emulated i6300esb on a clean
            # reboot ("watchdog did not stop!"), so it stays armed and resets the VM during the
            # next boot menu, causing a reboot loop. The target still ships RuntimeWatchdogSec
            # (inert here without a watchdog device); the happy/rollback scenarios rely on
            # boot-counting, not the watchdog, so dropping it does not reduce their coverage.
            "-rtc", "base=utc",
            "-display", "none", "-serial", "stdio", "-monitor", "none",
        ]

    def _read_loop(self):
        while True:
            chunk = self.proc.stdout.read(1)
            if not chunk:
                break
            text = chunk.decode("utf-8", errors="replace")
            with self.lock:
                self.buf += text
            self.logf.write(text)
            self.logf.flush()
            sys.stdout.write(text)
            sys.stdout.flush()

    def expect(self, patterns, timeout):
        """Wait until one of the regex patterns appears in newly-read output.

        Returns (index, before_text). Raises ConsoleError on timeout or QEMU exit.
        """
        if isinstance(patterns, str):
            patterns = [patterns]
        compiled = [re.compile(p) for p in patterns]
        deadline = time.time() + timeout
        while True:
            with self.lock:
                window = self.buf[self.pos:]
            best = None
            for i, rx in enumerate(compiled):
                m = rx.search(window)
                if m and (best is None or m.start() < best[1]):
                    best = (i, m.start(), m.end())
            if best is not None:
                idx, start, end = best
                before = window[:start]
                with self.lock:
                    self.pos += end
                return idx, before
            if self.proc.poll() is not None and not self._has_more():
                raise ConsoleError(
                    "%s: qemu exited (rc=%s) waiting for %r"
                    % (self.name, self.proc.returncode, patterns)
                )
            if time.time() > deadline:
                raise ConsoleError(
                    "%s: timeout after %ss waiting for %r" % (self.name, timeout, patterns)
                )
            time.sleep(0.2)

    def _has_more(self):
        with self.lock:
            return self.pos < len(self.buf)

    def send(self, line):
        self.proc.stdin.write((line + "\n").encode())
        self.proc.stdin.flush()

    def wait_exit(self, timeout):
        try:
            self.proc.wait(timeout=timeout)
            return True
        except Exception:
            return False

    def close(self):
        try:
            if self.proc.poll() is None:
                self.proc.terminate()
                self.proc.wait(timeout=15)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass
        try:
            self.logf.close()
        except Exception:
            pass


# --- High-level guest interactions --------------------------------------------------------

_MARK = [0]


def _next_marker():
    _MARK[0] += 1
    return "SBM%d" % _MARK[0]


def _emit(marker):
    # Print `marker` so the typed command's echo does NOT contain it literally (the embedded
    # quotes split the token), but the command's OUTPUT line does. This sidesteps prompt
    # theming/escape codes entirely: command output is plain text, prompts are not matched.
    return 'echo "SB""%s"' % marker[2:]


def wait_login(con, timeout=T_BOOT):
    """Wait until an autologin root shell is accepting commands.

    Works regardless of prompt theming/escape codes (grml-zsh on the live ISO, bash on the
    target) because it matches a sentinel we print, not the prompt.

    Each probe leads with a bare Enter: at a shell that is a harmless empty line, but at a
    waiting boot menu it immediately boots the highlighted default entry. This matters for
    GRUB, where any *printable* keystroke cancels the menu countdown and 'e' (the first
    letter of our echo probe) would drop into the entry editor; it also advances a
    systemd-boot menu left up by a failed (corrupt-kernel) boot.
    """
    deadline = time.time() + timeout
    while True:
        marker = _next_marker()
        con.send("")
        con.send(_emit(marker))
        try:
            con.expect([marker], timeout=8)
            return
        except ConsoleError:
            if con.proc.poll() is not None:
                raise
            if time.time() > deadline:
                raise ConsoleError("%s: timed out waiting for login shell" % con.name)


def sh(con, command, timeout=T_CMD):
    """Run a shell command; return the text printed before its end sentinel."""
    marker = _next_marker()
    con.send("%s; %s" % (command, _emit(marker)))
    _, before = con.expect([marker], timeout)
    return before


def answer(con, key, value, timeout=T_CMD):
    """Wait for the installer's prompt marker for `key`, then send `value`.

    Matches the trailing newline so a key that is a prefix of another
    (root-password vs root-password-confirm) cannot match the wrong marker.
    """
    con.expect([r"SILVERBLUE-INSTALL-PROMPT key=%s[\r\n]" % re.escape(key)], timeout)
    con.send(value)


def login_serial(con, password, host, timeout=T_BOOT):
    """Log in as root at a serial getty (interactive targets have no autologin).

    Do NOT call wait_login() first: its sentinel probes would be typed into the
    login: prompt. After the password we reuse the sentinel loop to confirm the
    shell — early probes may be swallowed while PAM runs, so keep retrying.
    """
    con.expect([r"%s login:" % re.escape(host)], timeout)
    con.send("root")
    con.expect([r"Password:"], T_CMD)
    con.send(password)
    deadline = time.time() + T_CMD
    while True:
        marker = _next_marker()
        con.send("")
        con.send(_emit(marker))
        try:
            con.expect([marker], timeout=8)
            return
        except ConsoleError:
            if con.proc.poll() is not None:
                raise
            if time.time() > deadline:
                raise ConsoleError("%s: timed out waiting for a shell after login" % con.name)


def get_subvol(con):
    out = sh(con, "cat /proc/cmdline")
    m = re.search(r"rootflags=subvol=(root-\S+)", out)
    if not m:
        raise ConsoleError("could not find rootflags=subvol in /proc/cmdline:\n%s" % out)
    return m.group(1).split(",")[0]


def run_update(con, tries=None):
    prefix = ("SB_TRIES=%d " % tries) if tries else ""
    out = sh(con, prefix + "%s-update" % BIN_PREFIX, timeout=T_UPDATE)
    if "==> Done." not in out:
        raise ConsoleError("%s-update did not complete cleanly:\n%s" % (BIN_PREFIX, out))
    m = re.search(r"new root\s*:\s*(root-\S+)", out)
    if not m:
        raise ConsoleError("could not parse new snapshot name from update output:\n%s" % out)
    return m.group(1)


def assert_markgood(con):
    # Wait for the unit to settle, then confirm it marked the boot good.
    unit = "%s-mark-good.service" % UNIT_PREFIX
    sh(con,
       "for i in $(seq 1 90); do "
       "systemctl is-active --quiet %s && break; "
       "systemctl is-failed --quiet %s && break; "
       "sleep 1; done" % (unit, unit),
       timeout=180)
    out = sh(con, "journalctl -u %s -b --no-pager" % unit)
    if "SILVERBLUE-MARKGOOD-OK" not in out or "SILVERBLUE-MARKGOOD-FAIL" in out:
        raise ConsoleError("mark-good did not report OK:\n%s" % out)


def assert_session(con):
    """Kiosk session came up: greetd active, sway + the console UI running, and the
    UI logged its READY marker (it renders on lavapipe here, so allow a slow start)."""
    ui = "%s-ui" % BIN_PREFIX
    sh(con,
       "for i in $(seq 1 150); do "
       "journalctl -t %s -b --no-pager 2>/dev/null | grep -q %s && break; "
       "sleep 2; done" % (ui, UI_READY_MARKER),
       timeout=400)
    out = sh(con, "systemctl is-active greetd")
    if "active" not in out:
        raise ConsoleError("greetd is not active:\n%s" % out)
    out = sh(con, "pgrep -x sway >/dev/null && pgrep -x %s >/dev/null && echo SESSION-PROCS-OK" % ui)
    if "SESSION-PROCS-OK" not in out:
        diag = sh(con, "journalctl -t %s -t %s-session -b --no-pager | tail -40" % (ui, BIN_PREFIX))
        raise ConsoleError("sway/%s not running:\n%s" % (ui, diag))
    out = sh(con, "journalctl -t %s -b --no-pager | grep %s | head -1" % (ui, UI_READY_MARKER))
    if UI_READY_MARKER not in out:
        raise ConsoleError("console UI never reported %s" % UI_READY_MARKER)


# --- Scenarios ----------------------------------------------------------------------------

def phase_install():
    con = Console("install", ["-cdrom", ISO])
    try:
        # The live ISO autologins root on the serial console (grml zsh). Wait for the shell to
        # accept commands, then drive the installer directly.
        wait_login(con, timeout=T_BOOT)
        con.send(
            "SB_SCENARIO=install SB_NET=%s SB_BOOTLOADER=%s bash /usr/local/bin/silverblue-autoinstall.sh"
            % (NET, BOOTLOADER)
        )
        idx, _ = con.expect(
            [r"SILVERBLUE-INSTALL-OK snap=(root-\S+)",
             r"SILVERBLUE-INSTALL-FAIL",
             r"SILVERBLUE-INSTALL-SKIP"],
            timeout=T_INSTALL,
        )
        if idx != 0:
            raise ConsoleError("install did not succeed")
        with con.lock:
            text = con.buf
        snap = re.search(r"SILVERBLUE-INSTALL-OK snap=(root-\S+)", text).group(1)
        con.wait_exit(timeout=120)
        return snap
    finally:
        con.close()


def phase_happy(initial_snap):
    con = Console("happy", [])
    try:
        wait_login(con)
        assert_markgood(con)
        print("\n[happy] initial root %s marked good" % initial_snap)
        if SESSION:
            assert_session(con)
            print("[happy] kiosk session is up (greetd + sway + %s-ui)" % BIN_PREFIX)

        new_snap = run_update(con)
        print("[happy] update produced new root %s; rebooting" % new_snap)
        con.send("sync; systemctl reboot")

        wait_login(con)
        booted = get_subvol(con)
        if booted != new_snap:
            raise ConsoleError("expected to boot %s after update, booted %s" % (new_snap, booted))
        assert_markgood(con)
        print("[happy] booted updated root %s and marked it good" % booted)

        con.send("poweroff")
        con.wait_exit(timeout=120)
        return True
    finally:
        con.close()


def phase_rollback():
    con = Console("rollback", [])
    try:
        wait_login(con)
        good = get_subvol(con)
        print("\n[rollback] current good root: %s" % good)

        bad_snap = run_update(con, tries=1)
        if bad_snap == good:
            raise ConsoleError("update did not create a distinct snapshot")
        if BOOTLOADER == "grub":
            # An unloadable kernel is systemd-boot's scenario (boot counting recovers it);
            # stock GRUB cannot recover that unattended — after a failed automatic boot it
            # waits at "Press any key" / the menu (see grub-helpers.sh). Instead exercise
            # the rollback mechanism GRUB does automate end-to-end: force the staged
            # root's health check to fail, so mark-good's OnFailure handler arms the
            # previous root and reboots into it.
            delay = 180 if SLOW else 45
            print("[rollback] bad update staged as %s; forcing its health check to fail" % bad_snap)
            sh(con,
               "d=$(findmnt -no SOURCE / | sed 's/\\[.*//'); "
               "mkdir -p /mnt/sbtop && mount -o subvolid=5 \"$d\" /mnt/sbtop && "
               "mkdir -p /mnt/sbtop/%s/etc/systemd/system/%s-mark-good.service.d && "
               "printf '[Service]\\nEnvironment=\"SB_HEALTHCHECK_CMD=sleep %d; exit 1\"\\n' "
               "> /mnt/sbtop/%s/etc/systemd/system/%s-mark-good.service.d/99-fail-health.conf && "
               "sync && umount /mnt/sbtop"
               % (bad_snap, UNIT_PREFIX, delay, bad_snap, UNIT_PREFIX))
        else:
            # systemd-boot only reads FAT, so each snapshot's kernel is copied onto the ESP.
            print("[rollback] bad update staged as %s; corrupting its kernel" % bad_snap)
            sh(con, "truncate -s 0 /efi/%s/%s/vmlinuz-linux; sync" % (ESP_SUBDIR, bad_snap))

        con.send("systemctl reboot")
        if BOOTLOADER == "grub":
            # The staged root boots normally and fails its health check `delay`s later.
            # Confirm the staged boot actually happened, then wait passively for the
            # rollback reboot's autologin banner — keystrokes at the GRUB menu would boot
            # an entry interactively and derail the automatic flow.
            con.expect([r"login: root \(automatic login\)"], timeout=T_BOOT)
            wait_login(con)
            staged = get_subvol(con)
            if staged != bad_snap:
                raise ConsoleError("expected staged boot of %s, got %s" % (bad_snap, staged))
            print("[rollback] staged root %s booted; awaiting health failure + auto-rollback" % staged)
            con.expect([r"login: root \(automatic login\)"], timeout=T_BOOT * 2)
        # systemd-boot tries the corrupt entry (tries=1 -> 0), fails, and falls back. wait_login
        # sends Enter each iteration, advancing any paused boot menu to the fallback entry.
        wait_login(con, timeout=T_BOOT * 2)
        landed = get_subvol(con)
        if landed == bad_snap:
            raise ConsoleError("system booted the corrupt root %s instead of rolling back" % bad_snap)
        if landed != good:
            raise ConsoleError("rolled back to %s, expected the previous good root %s" % (landed, good))
        print("[rollback] fell back to previous good root %s" % landed)

        con.send("poweroff")
        con.wait_exit(timeout=120)
        return True
    finally:
        con.close()


def phase_interactive_install():
    con = Console("interactive-install", ["-cdrom", ISO])
    try:
        wait_login(con, timeout=T_BOOT)
        # ANSWER ORDER IS A CONTRACT with gather_answers() in src/installer/silverblue-install.
        con.send("SB_INSTALL_MARKERS=1 %s-install" % BIN_PREFIX)
        answer(con, "disk", "1")                # the only candidate: the virtio test disk
        answer(con, "hostname", "sbtest")
        answer(con, "timezone", "")             # accept the distro.conf defaults
        answer(con, "locale", "")
        answer(con, "keymap", "")
        answer(con, "bootloader", "2" if BOOTLOADER == "grub" else "1")
        answer(con, "microcode", "n")           # keep the test target lean
        answer(con, "firmware", "n")
        answer(con, "network", "2")             # systemd-networkd
        answer(con, "root-password", ROOT_PW)
        answer(con, "root-password-confirm", ROOT_PW)
        answer(con, "username", "tester")
        answer(con, "user-password", USER_PW)
        answer(con, "user-password-confirm", USER_PW)
        answer(con, "confirm", "ERASE")
        idx, _ = con.expect(
            [r"SILVERBLUE-INSTALL-OK snap=(root-\S+)", r"SILVERBLUE-INSTALL-FAIL"],
            timeout=T_INSTALL,
        )
        if idx != 0:
            raise ConsoleError("interactive install failed")
        with con.lock:
            text = con.buf
        snap = re.search(r"SILVERBLUE-INSTALL-OK snap=(root-\S+)", text).group(1)
        answer(con, "reboot", "n")
        sh(con, "sync")
        con.send("poweroff -f")
        con.wait_exit(timeout=120)
        return snap
    finally:
        con.close()


def phase_interactive_boot(snap):
    con = Console("interactive-boot", [])
    try:
        login_serial(con, ROOT_PW, host="sbtest")
        booted = get_subvol(con)
        if booted != snap:
            raise ConsoleError("expected to boot %s, booted %s" % (snap, booted))
        out = sh(con, "cat /etc/hostname")
        if "sbtest" not in out:
            raise ConsoleError("unexpected hostname:\n%s" % out)
        assert_markgood(con)
        print("\n[interactive-boot] %s booted and marked good" % booted)

        out = sh(con, "systemctl is-enabled systemd-networkd.service")
        if "enabled" not in out:
            raise ConsoleError("systemd-networkd is not enabled:\n%s" % out)
        out = sh(con, "id -nG tester")
        if "wheel" not in out:
            raise ConsoleError("user tester missing or not in wheel:\n%s" % out)
        out = sh(con, "test -f /etc/sudoers.d/10-wheel && echo SUDOERS-PRESENT")
        if "SUDOERS-PRESENT" not in out:
            raise ConsoleError("sudoers drop-in missing:\n%s" % out)

        # Test-only artifacts of the unattended appliance must NOT exist here.
        out = sh(con, "test ! -e /etc/systemd/system/serial-getty@ttyS0.service.d/autologin.conf"
                      " && echo NO-AUTOLOGIN")
        if "NO-AUTOLOGIN" not in out:
            raise ConsoleError("autologin drop-in leaked onto an interactive target")
        out = sh(con, "test ! -d /opt/silverblue && echo NO-TESTREPO")
        if "NO-TESTREPO" not in out:
            raise ConsoleError("local test repo leaked onto an interactive target")
        print("[interactive-boot] user/network/no-test-artifacts checks passed")

        # Diagnostic only: GRUB installs rely on mkinitcpio's `microcode` hook embedding
        # ucode into the initramfs (systemd-boot lists *-ucode.img explicitly).
        out = sh(con, "grep ^HOOKS /etc/mkinitcpio.conf")
        print("[interactive-boot] target mkinitcpio %s" % out.strip().splitlines()[-1]
              if out.strip() else "[interactive-boot] no HOOKS line found")

        con.send("poweroff")
        con.wait_exit(timeout=120)
        return True
    finally:
        con.close()


def main():
    results = []
    if INTERACTIVE:
        try:
            snap = phase_interactive_install()
            results.append(("interactive-install", True, "installed %s" % snap))
        except ConsoleError as e:
            results.append(("interactive-install", False, str(e)))
            return report(results)
        try:
            phase_interactive_boot(snap)
            results.append(("interactive-boot", True, "ok"))
        except ConsoleError as e:
            results.append(("interactive-boot", False, str(e)))
        return report(results)

    try:
        snap = phase_install()
        results.append(("install", True, "installed %s" % snap))
    except ConsoleError as e:
        results.append(("install", False, str(e)))
        return report(results)

    for name, fn, arg in (("happy", phase_happy, snap), ("rollback", phase_rollback, None)):
        try:
            fn(arg) if arg is not None else fn()
            results.append((name, True, "ok"))
        except ConsoleError as e:
            results.append((name, False, str(e)))

    return report(results)


def report(results):
    print("\n================ QEMU TEST SUMMARY ================")
    ok = True
    for name, passed, detail in results:
        print("  %-9s %s  %s" % (name, "PASS" if passed else "FAIL", detail))
        ok = ok and passed
    print("===================================================")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
