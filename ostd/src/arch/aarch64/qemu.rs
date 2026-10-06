// SPDX-License-Identifier: MPL-2.0

//! Providing the ability to exit QEMU and return a value as debug result.

/// The exit code of QEMU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QemuExitCode {
    /// The code that indicates a successful exit.
    Success,
    /// The code that indicates a failed exit.
    Failed,
}

/// Invoke PSCI SYSTEM_RESET through the firmware monitor.
pub fn psci_system_reset() -> ! {
    const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;
    unsafe {
        if super::is_rpi3() {
            core::arch::asm!("smc #0", in("x0") PSCI_SYSTEM_RESET);
        } else {
            core::arch::asm!("hvc #0", in("x0") PSCI_SYSTEM_RESET);
        }
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Invoke PSCI SYSTEM_OFF through the firmware monitor.
pub fn psci_system_off() -> ! {
    const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
    unsafe {
        if super::is_rpi3() {
            core::arch::asm!("smc #0", in("x0") PSCI_SYSTEM_OFF);
        } else {
            core::arch::asm!("hvc #0", in("x0") PSCI_SYSTEM_OFF);
        }
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Exit QEMU with the given exit code.
pub fn exit_qemu(_exit_code: QemuExitCode) -> ! {
    const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
    unsafe {
        // Try PSCI SYSTEM_OFF via HVC (standard for QEMU virt without ATF).
        core::arch::asm!(
            "hvc #0",
            in("x0") PSCI_SYSTEM_OFF,
            options(noreturn)
        );
    }

    loop {
        core::hint::spin_loop();
    }
}
