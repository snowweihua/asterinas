// SPDX-License-Identifier: MPL-2.0

use super::SyscallReturn;
use crate::{context::Context, prelude::*};

const LINUX_REBOOT_MAGIC1: u32 = 0xfee1_dead;
const LINUX_REBOOT_MAGIC2: u32 = 0x2812_1969;
const LINUX_REBOOT_CMD_RESTART: u32 = 0x1234_567;
const LINUX_REBOOT_CMD_HALT: u32 = 0x4321_efcd;
const LINUX_REBOOT_CMD_POWER_OFF: u32 = 0x4321_1234;
const LINUX_REBOOT_CMD_RESTART2: u32 = 0xa1b2_c3d4;
const LINUX_REBOOT_CMD_SW_SUSPEND: u32 = 0x2400_0003;

#[cfg(target_arch = "aarch64")]
fn platform_reboot() {
    ostd::arch::qemu::psci_system_reset();
}

#[cfg(not(target_arch = "aarch64"))]
fn platform_reboot() {}

#[cfg(target_arch = "aarch64")]
fn platform_poweroff() {
    ostd::arch::qemu::psci_system_off();
}

#[cfg(not(target_arch = "aarch64"))]
fn platform_poweroff() {}

pub fn sys_reboot(
    magic1: i32,
    magic2: i32,
    cmd: i32,
    _arg: usize,
    _ctx: &Context,
) -> Result<SyscallReturn> {
    debug!(
        "sys_reboot magic1={:#x} magic2={:#x} cmd={:#x}",
        magic1, magic2, cmd
    );

    if (magic1 as u32) != LINUX_REBOOT_MAGIC1 || (magic2 as u32) != LINUX_REBOOT_MAGIC2 {
        return_errno_with_message!(Errno::EINVAL, "invalid reboot magic numbers");
    }

    match cmd as u32 {
        LINUX_REBOOT_CMD_RESTART | LINUX_REBOOT_CMD_RESTART2 => {
            platform_reboot();
            return_errno_with_message!(Errno::ENOSYS, "PSCI SYSTEM_RESET not supported");
        }
        LINUX_REBOOT_CMD_POWER_OFF => {
            platform_poweroff();
            return_errno_with_message!(Errno::ENOSYS, "PSCI SYSTEM_OFF not supported");
        }
        LINUX_REBOOT_CMD_HALT => {
            loop {
                core::hint::spin_loop();
            }
        }
        LINUX_REBOOT_CMD_SW_SUSPEND => {
            return_errno_with_message!(Errno::ENOSYS, "suspend not implemented");
        }
        _ => {
            return_errno_with_message!(Errno::EINVAL, "unrecognized reboot command");
        }
    }
}
