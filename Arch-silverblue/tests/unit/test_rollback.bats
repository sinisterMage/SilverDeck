#!/usr/bin/env bats
# silverblue-rollback.sh: a successful rollback arms the previous root and reboots; a *failed*
# rollback (no previous snapshot) must NOT reboot — that would loop straight back into the same
# failing root — and instead drops to an emergency shell.
#
# The script is sourceable (it only runs rollback_main when executed directly). We point SB_ENGINE
# at a tiny stub and record systemctl verbs to a file instead of touching the real init system.

load helper

setup() {
    TMP="$(mktemp -d)"
    SYSTEMCTL_LOG="$TMP/systemctl.log"
    : > "$SYSTEMCTL_LOG"

    ENGINE_OK="$TMP/engine-ok"
    ENGINE_FAIL="$TMP/engine-fail"
    printf '#!/usr/bin/env bash\nexit 0\n' > "$ENGINE_OK"
    printf '#!/usr/bin/env bash\nexit 1\n' > "$ENGINE_FAIL"
    chmod +x "$ENGINE_OK" "$ENGINE_FAIL"

    # shellcheck source=/dev/null
    source "$SB_REPO/src/init/silverblue-rollback.sh"
}

teardown() { rm -rf "$TMP"; }

# Record systemctl verbs instead of rebooting / isolating the test host.
systemctl() { printf '%s\n' "$*" >> "$SYSTEMCTL_LOG"; }

@test "successful rollback arms the previous root and reboots" {
    SB_ENGINE="$ENGINE_OK"
    run rollback_main
    [ "$status" -eq 0 ]
    [[ "$output" == *SILVERBLUE-ROLLBACK-ARMED* ]]
    grep -qx reboot "$SYSTEMCTL_LOG"
    ! grep -q emergency "$SYSTEMCTL_LOG"
}

@test "failed rollback drops to emergency instead of rebooting (no loop)" {
    SB_ENGINE="$ENGINE_FAIL"
    run rollback_main
    [ "$status" -eq 1 ]
    [[ "$output" == *SILVERBLUE-ROLLBACK-FAILED-NO-REBOOT* ]]
    grep -qx emergency "$SYSTEMCTL_LOG"
    ! grep -q reboot "$SYSTEMCTL_LOG"
}

@test "SB_NO_REBOOT suppresses the emergency transition for isolated testing" {
    SB_ENGINE="$ENGINE_FAIL"
    SB_NO_REBOOT=1
    run rollback_main
    [ "$status" -eq 1 ]
    [[ "$output" == *SILVERBLUE-ROLLBACK-FAILED-NO-REBOOT* ]]
    [ ! -s "$SYSTEMCTL_LOG" ]
}
