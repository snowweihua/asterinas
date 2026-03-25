// SPDX-License-Identifier: MPL-2.0

//! The architecture support of context switch.

use crate::task::TaskContextApi;

core::arch::global_asm!(include_str!("switch.S"));

#[derive(Debug, Clone)]
#[repr(C)]
pub(crate) struct TaskContext {
    regs: CalleeRegs,
    ra: usize,
}

impl TaskContext {
    /// Creates a new `TaskContext`.
    pub(crate) const fn new() -> Self {
        TaskContext {
            regs: CalleeRegs::new(),
            ra: 0,
        }
    }
}

/// Callee-saved registers.
#[derive(Debug, Clone)]
#[repr(C)]
struct CalleeRegs {
    x19: u64,
    x20: u64,
    x21: u64,
    x22: u64,
    x23: u64,
    x24: u64,
    x25: u64,
    x26: u64,
    x27: u64,
    x28: u64,
    x29: u64,  // fp
    x30: u64,  // lr
    tpidr_el0: u64,
    sp: u64,
}

impl CalleeRegs {
    /// Creates a new `CalleeRegs`.
    pub(self) const fn new() -> Self {
        CalleeRegs {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: 0,
            tpidr_el0: 0,
            sp: 0,
        }
    }
}

impl TaskContextApi for TaskContext {
    fn set_instruction_pointer(&mut self, ip: usize) {
        // AArch64 context switch uses `ret` to jump to x30 (lr).
        // The assembly restores x30 from offset 88 (regs.x30), so we must
        // write the task entry address there, not to the `ra` field.
        self.regs.x30 = ip as u64;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.regs.sp = sp as u64;
    }
}

unsafe extern "C" {
    pub(crate) fn context_switch(nxt: *const TaskContext, cur: *mut TaskContext);
    pub(crate) fn first_context_switch(nxt: *const TaskContext);
    pub(crate) fn kernel_task_entry_wrapper();
}
