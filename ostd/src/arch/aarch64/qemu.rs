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

/// Exit QEMU with the given exit code.
pub fn exit_qemu(_exit_code: QemuExitCode) -> ! {
    // Direct UART marker to confirm we reached exit_qemu (bypasses log framework).
    crate::arch::boot::pl011_puts_static(b"EQ\n");
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
