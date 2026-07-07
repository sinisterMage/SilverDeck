#!/usr/bin/env bats
# Snapshot listing, prune planning, and previous-snapshot selection.

load helper

setup() {
    load_engine
    TMP="$(mktemp -d)"
}

teardown() { rm -rf "$TMP"; }

@test "prune_plan deletes the oldest beyond keep" {
    result="$(printf 'root-1\nroot-2\nroot-3\nroot-4\nroot-5\n' | prune_plan 3)"
    [ "$result" = "$(printf 'root-1\nroot-2')" ]
}

@test "prune_plan keeps everything when at or under keep" {
    result="$(printf 'root-1\nroot-2\nroot-3\n' | prune_plan 3)"
    [ -z "$result" ]
}

@test "prune_plan keep=1 leaves only the newest" {
    result="$(printf 'a\nb\nc\n' | prune_plan 1)"
    [ "$result" = "$(printf 'a\nb')" ]
}

@test "list_snapshots returns sorted root-* directories only" {
    mkdir -p "$TMP/root-20260301-000000" "$TMP/root-20260101-000000" \
             "$TMP/root-20260201-000000" "$TMP/notasnapshot"
    run list_snapshots "$TMP"
    [ "${lines[0]}" = "root-20260101-000000" ]
    [ "${lines[1]}" = "root-20260201-000000" ]
    [ "${lines[2]}" = "root-20260301-000000" ]
    [ "${#lines[@]}" -eq 3 ]
}

@test "previous_snapshot picks the newest non-current snapshot" {
    mkdir -p "$TMP/root-20260101-000000" "$TMP/root-20260201-000000" "$TMP/root-20260301-000000"
    SB_TOPLEVEL_MNT="$TMP"
    CURRENT_SUBVOL="root-20260301-000000"
    run previous_snapshot
    [ "$output" = "root-20260201-000000" ]
}
