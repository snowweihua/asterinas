// SPDX-License-Identifier: MPL-2.0 OR MIT
//
// The original source code is from [trapframe-rs](https://github.com/rcore-os/trapframe-rs),
// which is released under the following license:
//
// SPDX-License-Identifier: MIT
//
// Copyright (c) 2020 - 2024 Runji Wang
//
// We make the following new changes:
// * Implement the `trap_handler` of Asterinas.
//
// These changes are released under the following license:
//
// SPDX-License-Identifier: MPL-2.0

#![allow(unfulfilled_lint_expectations)]

use core::arch::{asm, global_asm};

use crate::arch::cpu::context::GeneralRegs;

#[cfg(target_arch = "aarch64")]
global_asm!(include_str!("trap.S"));

/// Initialize interrupt handling for the current HART.
///
/// # Safety
///
/// This function will:
/// - Set `sscratch` to 0.
/// - Set `stvec` to internal exception vector.
///
/// You **MUST NOT** modify these registers later.
pub unsafe fn init() {
    unsafe {
        asm!(
            "adr x9, vector_table_el1",
            "msr vbar_el1, x9",
            "adr x9, vector_table_el2",
            "msr vbar_el2, x9",
            "adr x9, vector_table_el3",
            "msr vbar_el3, x9",
            options(nomem, nostack),
            out("x9") _,
        );
    }
}

/// Trap frame of kernel interrupt
///
/// # Trap handler
///
/// You need to define a handler function like this:
///
/// ```no_run
/// #[no_mangle]
/// pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
///     println!("TRAP! tf: {:#x?}", tf);
/// }
/// ```
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct TrapFrame {
    /// General registers
    pub general: GeneralRegs,
    /// link register, aka x30
    pub lr: usize,
    /// exception link register
    pub elr_el1: usize,
    /// saved program status
    pub spsr_el1: usize,
    /// exception syndrome register
    pub esr_el1: usize,
}

/// Saved registers on a trap.
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub(in crate::arch) struct RawUserContext {
    /// General registers
    pub(in crate::arch) general: GeneralRegs,
    /// link register
    pub(in crate::arch) lr: usize,
    /// exception link register
    pub(in crate::arch) elr_el1: usize,
    /// saved program status
    pub(in crate::arch) spsr_el1: usize,
    /// exception syndrome register
    pub(in crate::arch) esr_el1: usize,
}

impl RawUserContext {
    /// Goes to user space with the context, and comes back when a trap occurs.
    ///
    /// On return, the context will be reset to the status before the trap.
    /// Trap reason and error code will be placed at `scause` and `stval`.
    pub(in crate::arch) fn run(&mut self) {
        // Return to userspace with interrupts disabled. Otherwise, interrupts
        // after switching `sscratch` will mess up the CPU state.
        crate::arch::irq::disable_local();
        unsafe { run_user(self) }
    }
}

#[expect(improper_ctypes)]
unsafe extern "C" {
    fn trap_entry();
    fn run_user(regs: &mut RawUserContext);
}
