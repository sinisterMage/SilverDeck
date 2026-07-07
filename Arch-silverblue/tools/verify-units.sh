#!/usr/bin/env bash
#
# verify-units.sh — run `systemd-analyze verify` on the Silverblue systemd units.
#
# The units reference absolute ExecStart paths (/usr/lib/silverblue/...) that only exist once
# installed, so verifying them on a dev host where Silverblue isn't installed would spuriously
# fail. This stages the units + their scripts at their installed paths under a throwaway root
# (plus stub copies of the host's standard target units) and verifies with `--root`, so the
# check is meaningful and passes on any host that has systemd-analyze.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

command -v systemd-analyze >/dev/null 2>&1 || { echo "systemd-analyze not found" >&2; exit 1; }

DEST=$(mktemp -d)
trap 'rm -rf "$DEST"' EXIT

# Stage the Silverblue tree at its installed absolute paths.
install -Dm0755 "$ROOT/src/update-engine/silverblue-update"   "$DEST/usr/bin/silverblue-update"
install -Dm0644 "$ROOT/src/bootloader/sdboot-helpers.sh"      "$DEST/usr/lib/silverblue/sdboot-helpers.sh"
install -Dm0644 "$ROOT/src/bootloader/grub-helpers.sh"        "$DEST/usr/lib/silverblue/grub-helpers.sh"
install -Dm0755 "$ROOT/src/init/silverblue-mark-good.sh"      "$DEST/usr/lib/silverblue/silverblue-mark-good.sh"
install -Dm0755 "$ROOT/src/init/silverblue-rollback.sh"       "$DEST/usr/lib/silverblue/silverblue-rollback.sh"
for u in silverblue-mark-good.service silverblue-rollback.service silverblue-rollback.target; do
    install -Dm0644 "$ROOT/src/init/$u" "$DEST/usr/lib/systemd/system/$u"
done

# Stage stub copies of the host's standard target units so dependency resolution succeeds
# under --root. Targets carry no ExecStart, so copying them introduces no missing-binary noise.
stub_src=""
for d in /usr/lib/systemd/system /etc/systemd/system /run/systemd/system /lib/systemd/system; do
    if compgen -G "$d/*.target" >/dev/null 2>&1; then stub_src=$d; break; fi
done
[[ -n "$stub_src" ]] || { echo "could not find standard systemd target units to stage" >&2; exit 1; }
for t in "$stub_src"/*.target; do
    cp -Lf "$t" "$DEST/usr/lib/systemd/system/$(basename "$t")" 2>/dev/null || true
done

rc=0
for unit in silverblue-mark-good.service silverblue-rollback.service silverblue-rollback.target; do
    echo "==> systemd-analyze verify $unit"
    if systemd-analyze verify --root="$DEST" "/usr/lib/systemd/system/$unit"; then
        echo "    OK"
    else
        echo "    FAILED" >&2
        rc=1
    fi
done
exit "$rc"
