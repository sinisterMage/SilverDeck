#!/usr/bin/env bats
# health_check decision logic in silverblue-mark-good.sh.
#
# The script is sourceable (it only runs mark_good_main when executed directly), so we source it
# here — which also sources the engine for vlog/err — and call health_check against a stubbed
# systemctl. No root, no real init system.

load helper

setup() {
    # shellcheck source=/dev/null
    source "$SB_REPO/src/init/silverblue-mark-good.sh"
}

# Stub systemctl: `is-system-running` echoes $FAKE_STATE; `is-active` reports every unit active
# except $FAKE_INACTIVE_UNIT. `-` (not `:-`) so an explicitly empty FAKE_STATE stays empty.
systemctl() {
    case "$1" in
        is-system-running) printf '%s\n' "${FAKE_STATE-running}" ;;
        is-active)         [[ "$2" == "${FAKE_INACTIVE_UNIT:-}" ]] && printf 'inactive\n' || printf 'active\n' ;;
        *)                 return 0 ;;
    esac
}

@test "healthy when system is running" {
    FAKE_STATE=running
    run health_check
    [ "$status" -eq 0 ]
}

@test "healthy when starting" {
    FAKE_STATE=starting
    run health_check
    [ "$status" -eq 0 ]
}

@test "healthy when initializing" {
    FAKE_STATE=initializing
    run health_check
    [ "$status" -eq 0 ]
}

@test "degraded is healthy when the critical units are active" {
    FAKE_STATE=degraded
    run health_check
    [ "$status" -eq 0 ]
}

@test "degraded is unhealthy when local-fs.target is down" {
    FAKE_STATE=degraded
    FAKE_INACTIVE_UNIT=local-fs.target
    run health_check
    [ "$status" -eq 1 ]
}

@test "degraded is unhealthy when sysinit.target is down" {
    FAKE_STATE=degraded
    FAKE_INACTIVE_UNIT=sysinit.target
    run health_check
    [ "$status" -eq 1 ]
}

@test "an unexpected state (stopping) is unhealthy" {
    FAKE_STATE=stopping
    run health_check
    [ "$status" -eq 1 ]
}

@test "an empty/unknown state is unhealthy" {
    FAKE_STATE=
    run health_check
    [ "$status" -eq 1 ]
}

@test "SB_HEALTHCHECK_CMD overrides the check (success)" {
    SB_HEALTHCHECK_CMD=true
    run health_check
    [ "$status" -eq 0 ]
}

@test "SB_HEALTHCHECK_CMD overrides the check (failure)" {
    SB_HEALTHCHECK_CMD=false
    run health_check
    [ "$status" -eq 1 ]
}
