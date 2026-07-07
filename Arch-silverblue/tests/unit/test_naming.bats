#!/usr/bin/env bats
# Snapshot naming, labelling, and kernel-cmdline parsing.

load helper

setup() { load_engine; }

@test "now_ts honors SB_NOW for determinism" {
    SB_NOW=20260628-093000
    run now_ts
    [ "$status" -eq 0 ]
    [ "$output" = "20260628-093000" ]
}

@test "next_snapshot_name builds the root- prefix" {
    SB_NOW=20260628-093000
    run next_snapshot_name
    [ "$output" = "root-20260628-093000" ]
}

@test "label_for is human readable" {
    run label_for "root-20260628-093000"
    [ "$output" = "Arch Silverblue 2026-06-28 09:30" ]
}

@test "parse_subvol_from_cmdline extracts the subvolume" {
    run parse_subvol_from_cmdline "root=UUID=abc rootflags=subvol=root-20260628-093000 rw quiet"
    [ "$status" -eq 0 ]
    [ "$output" = "root-20260628-093000" ]
}

@test "parse_subvol_from_cmdline handles extra rootflags options" {
    run parse_subvol_from_cmdline "rootflags=subvol=root-X,compress=zstd ro"
    [ "$output" = "root-X" ]
}

@test "parse_subvol_from_cmdline fails when no subvol present" {
    run parse_subvol_from_cmdline "root=UUID=abc rw"
    [ "$status" -ne 0 ]
}
