# SPDX-License-Identifier: MPL-2.0
# Static reboot helper for AArch64 initramfs.
# Invokes the Linux reboot(2) syscall with LINUX_REBOOT_CMD_RESTART so that
# PSCI SYSTEM_RESET is reached without depending on glibc/dynamic busybox.

.section .text
.global _start
_start:
    // reboot(LINUX_REBOOT_MAGIC1, LINUX_REBOOT_MAGIC2, LINUX_REBOOT_CMD_RESTART, 0)
    ldr x0, =0xfee1dead
    ldr x1, =0x28121969
    ldr x2, =0x1234567
    mov x3, #0
    mov x8, #142
    svc #0
    // If the syscall returns, hang in a tight loop.
1:
    b 1b
