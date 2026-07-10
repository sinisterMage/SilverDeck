#!/usr/bin/env bash
#
# build-repo.sh — build the SilverDeck pacman packages in the Arch builder
# container and publish them as a local repo at
# Arch-silverblue/iso/local-repo/, where iso/build.sh bakes them into
# the ISO (and the installer copies them onto the target).
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT="$ROOT/Arch-silverblue/iso/local-repo"
IMAGE=silverdeck-builder

log() { printf '[build-repo] %s\n' "$*"; }

log "Building builder image"
docker build -t "$IMAGE" -f "$ROOT/packaging/Dockerfile.builder" "$ROOT/packaging"

mkdir -p "$OUT"
rm -f "$OUT"/*.pkg.tar.zst "$OUT"/silverdeck.db* "$OUT"/silverdeck.files*

# A named volume keeps the cargo registry/artifacts across runs.
log "Building packages (cargo cache volume: silverdeck-cargo)"
docker run --rm \
    -v "$ROOT:/src:ro" \
    -v "$OUT:/out" \
    -v silverdeck-cargo:/home/builder/.cargo \
    "$IMAGE" bash -ec '
        for pkg in silverdeck-ui silverdeck-installer silverdeck-session silverdeck-plymouth; do
            cp -r "/src/packaging/$pkg" "/tmp/$pkg"
            cd "/tmp/$pkg"
            # -d: runtime deps (greetd, sway, ...) resolve on the target at pacstrap
            # time; the builder image only carries the make-deps.
            SILVERDECK_SRC=/src PKGDEST=/out BUILDDIR=/tmp/build makepkg -f -d --noconfirm
        done
        cd /out && repo-add --new silverdeck.db.tar.gz ./*.pkg.tar.zst
    '

log "Done:"
ls -l "$OUT"
