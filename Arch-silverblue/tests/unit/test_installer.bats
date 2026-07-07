#!/usr/bin/env bats
# Pure helpers of the shared installer library (install-lib.sh).

load helper

setup() { load_installer_lib; }

@test "esc escapes sed replacement metacharacters" {
    run esc 'a|b&c\d'
    [ "$output" = 'a\|b\&c\\d' ]
}

@test "validate_hostname accepts sane names" {
    validate_hostname "silverblue"
    validate_hostname "my-box2"
    validate_hostname "A1"
}

@test "validate_hostname rejects bad names" {
    run validate_hostname "-bad"
    [ "$status" -ne 0 ]
    run validate_hostname "bad-"
    [ "$status" -ne 0 ]
    run validate_hostname "with space"
    [ "$status" -ne 0 ]
    run validate_hostname ""
    [ "$status" -ne 0 ]
}

@test "validate_username accepts sane names" {
    validate_username "tester"
    validate_username "_svc"
    validate_username "user-1"
}

@test "validate_username rejects bad names" {
    run validate_username "Tester"
    [ "$status" -ne 0 ]
    run validate_username "1user"
    [ "$status" -ne 0 ]
    run validate_username ""
    [ "$status" -ne 0 ]
}

@test "detect_microcode identifies Intel" {
    run detect_microcode "$(printf 'processor : 0\nvendor_id : GenuineIntel\n')"
    [ "$output" = "intel-ucode" ]
}

@test "detect_microcode identifies AMD" {
    run detect_microcode "$(printf 'processor : 0\nvendor_id : AuthenticAMD\n')"
    [ "$output" = "amd-ucode" ]
}

@test "detect_microcode is silent on unknown vendors" {
    run detect_microcode "$(printf 'processor : 0\nvendor_id : CyrixInstead\n')"
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "filter_disks keeps real disks and drops live medium, rom, loop, zram" {
    local listing='PATH="/dev/vda" TYPE="disk" SIZE="12G" MODEL=""
PATH="/dev/nvme0n1" TYPE="disk" SIZE="931.5G" MODEL="Samsung SSD 980"
PATH="/dev/sda" TYPE="disk" SIZE="14.9G" MODEL="USB Flash"
PATH="/dev/sr0" TYPE="rom" SIZE="800M" MODEL="QEMU DVD-ROM"
PATH="/dev/loop0" TYPE="loop" SIZE="700M" MODEL=""
PATH="/dev/zram0" TYPE="disk" SIZE="4G" MODEL=""'
    run filter_disks "$listing" "/dev/sda"
    [ "$status" -eq 0 ]
    [ "${#lines[@]}" -eq 2 ]
    [ "${lines[0]}" = "/dev/vda|12G|" ]
    [ "${lines[1]}" = "/dev/nvme0n1|931.5G|Samsung SSD 980" ]
}

@test "filter_disks with no live disk keeps everything installable" {
    local listing='PATH="/dev/sda" TYPE="disk" SIZE="14.9G" MODEL="USB Flash"'
    run filter_disks "$listing" ""
    [ "${lines[0]}" = "/dev/sda|14.9G|USB Flash" ]
}

@test "partition_path appends the number for letter-named disks" {
    run partition_path /dev/vda 1
    [ "$output" = "/dev/vda1" ]
    run partition_path /dev/sda 2
    [ "$output" = "/dev/sda2" ]
}

@test "partition_path inserts p for digit-named disks" {
    run partition_path /dev/nvme0n1 1
    [ "$output" = "/dev/nvme0n1p1" ]
    run partition_path /dev/mmcblk0 2
    [ "$output" = "/dev/mmcblk0p2" ]
}

@test "console_opts_for_tty carries a serial console" {
    run console_opts_for_tty /dev/ttyS0
    [ "$output" = "console=ttyS0,115200 console=tty0" ]
}

@test "console_opts_for_tty is empty for virtual terminals" {
    run console_opts_for_tty /dev/tty1
    [ "$status" -eq 0 ]
    [ -z "$output" ]
    run console_opts_for_tty "not a tty"
    [ -z "$output" ]
}

@test "render_summary reflects the chosen answers" {
    SB_INST_DISK=/dev/vda
    SB_INST_HOSTNAME=sbtest
    SB_INST_BOOTLOADER=grub
    SB_INST_NETWORK=networkd
    run render_summary
    [ "$status" -eq 0 ]
    [[ "$output" == *"/dev/vda"* ]]
    [[ "$output" == *"sbtest"* ]]
    [[ "$output" == *"grub"* ]]
    [[ "$output" == *"networkd"* ]]
}
