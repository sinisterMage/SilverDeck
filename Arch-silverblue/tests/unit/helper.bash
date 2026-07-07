# shellcheck shell=bash
# Common bats setup: locate the repo and source the update engine so tests can call its
# pure functions directly. The engine only defines functions when sourced (BASH_SOURCE != $0),
# and its state-changing wrapper is sb_run (not run), so bats's own run() is left intact.

SB_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export SB_REPO
export SB_LIB_DIR="$SB_REPO/src/bootloader"

load_engine() {
    # shellcheck source=/dev/null
    source "$SB_REPO/src/update-engine/silverblue-update"
}

load_installer_lib() {
    # shellcheck source=/dev/null
    source "$SB_REPO/src/installer/install-lib.sh"
}
