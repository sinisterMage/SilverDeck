#!/usr/bin/env bats
# Prompt helpers of the shared installer library: stdin-driven, result on stdout,
# prompt text on stderr, SILVERBLUE-INSTALL-PROMPT markers behind SB_INSTALL_MARKERS=1.

load helper

setup() { load_installer_lib; }

@test "ask returns the typed value" {
    result=$(ask hostname "Hostname" "fallback" 2>/dev/null <<< "mybox")
    [ "$result" = "mybox" ]
}

@test "ask returns the default on empty input" {
    result=$(ask hostname "Hostname" "fallback" 2>/dev/null <<< "")
    [ "$result" = "fallback" ]
}

@test "ask re-prompts until the validator accepts" {
    result=$(ask hostname "Hostname" "" validate_hostname 2>/dev/null <<< $'-bad-\ngood-name')
    [ "$result" = "good-name" ]
}

@test "ask fails on end of input" {
    run ask hostname "Hostname" "" validate_hostname <<< "-bad-"
    [ "$status" -ne 0 ]
}

@test "ask_yesno takes the default on empty input" {
    run ask_yesno ok "OK?" y <<< ""
    [ "$status" -eq 0 ]
    run ask_yesno ok "OK?" n <<< ""
    [ "$status" -eq 1 ]
}

@test "ask_yesno accepts explicit answers" {
    run ask_yesno ok "OK?" y <<< "n"
    [ "$status" -eq 1 ]
    run ask_yesno ok "OK?" n <<< "yes"
    [ "$status" -eq 0 ]
}

@test "ask_yesno re-prompts on garbage" {
    run ask_yesno ok "OK?" n <<< $'maybe\ny'
    [ "$status" -eq 0 ]
}

@test "choose maps a number to its item" {
    result=$(choose bootloader "Bootloader:" "systemd-boot" systemd-boot grub 2>/dev/null <<< "2")
    [ "$result" = "grub" ]
}

@test "choose returns the default on empty input" {
    result=$(choose bootloader "Bootloader:" "systemd-boot" systemd-boot grub 2>/dev/null <<< "")
    [ "$result" = "systemd-boot" ]
}

@test "choose re-prompts on an out-of-range selection" {
    result=$(choose pick "Pick:" "a" a b 2>/dev/null <<< $'9\n1')
    [ "$result" = "a" ]
}

@test "ask_secret accepts a matching pair" {
    result=$(ask_secret root-password "Root password" 2>/dev/null <<< $'secret\nsecret')
    [ "$result" = "secret" ]
}

@test "ask_secret loops on mismatch, then accepts" {
    result=$(ask_secret root-password "Root password" 2>/dev/null <<< $'one\ntwo\nsecret\nsecret')
    [ "$result" = "secret" ]
}

@test "ask_secret rejects empty passwords" {
    result=$(ask_secret root-password "Root password" 2>/dev/null <<< $'\n\nsecret\nsecret')
    [ "$result" = "secret" ]
}

@test "prompt markers are emitted when SB_INSTALL_MARKERS=1" {
    SB_INSTALL_MARKERS=1
    run ask hostname "Hostname" "x" <<< ""
    [[ "$output" == *"SILVERBLUE-INSTALL-PROMPT key=hostname"* ]]
}

@test "ask_secret emits both key and key-confirm markers" {
    SB_INSTALL_MARKERS=1
    run ask_secret root-password "Root password" <<< $'secret\nsecret'
    [[ "$output" == *"SILVERBLUE-INSTALL-PROMPT key=root-password"* ]]
    [[ "$output" == *"SILVERBLUE-INSTALL-PROMPT key=root-password-confirm"* ]]
}

@test "prompt markers are silent by default" {
    run ask hostname "Hostname" "x" <<< ""
    [[ "$output" != *"SILVERBLUE-INSTALL-PROMPT"* ]]
}
