#!/usr/bin/env bash
#
# silverblue-rollback.sh — arm a rollback to the previous snapshot and reboot.
#
# Invoked by silverblue-rollback.service, which is started via the OnFailure= handler of
# silverblue-mark-good.service when a boot fails its post-boot health check.
#
# Like the engine, this file is sourceable: it only runs rollback_main (and enables strict mode)
# when executed directly, so the unit tests can source it and exercise rollback_main in isolation.

rollback_main() {
    local engine=${SB_ENGINE:-/usr/bin/silverblue-update}
    [[ -x "$engine" ]] || engine=silverblue-update

    printf 'SILVERBLUE-ROLLBACK-ARMING\n'
    if "$engine" --rollback; then
        printf 'SILVERBLUE-ROLLBACK-ARMED\n'
        # Reboot into the previous (known-good) root. Honor SB_NO_REBOOT for testing in isolation.
        if [[ -z "${SB_NO_REBOOT:-}" ]]; then
            systemctl reboot
        fi
        return 0
    fi

    # The rollback could not be armed (e.g. there is no previous snapshot to fall back to).
    # Rebooting now would land us right back in the same failing root and loop forever, so drop
    # to an emergency shell instead and let an operator intervene.
    printf 'SILVERBLUE-ROLLBACK-FAILED-NO-REBOOT (no previous snapshot?)\n' >&2
    if [[ -z "${SB_NO_REBOOT:-}" ]]; then
        systemctl emergency
    fi
    return 1
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    set -uo pipefail
    rollback_main "$@"
fi
