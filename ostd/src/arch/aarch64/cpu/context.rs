// SPDX-License-Identifier: MPL-2.0

//! CPU execution context control.

use core::{fmt::Debug, sync::atomic::Ordering};

use riscv::register::scause::{Exception, Interrupt, Trap};
use aarch64_cpu::registers::{ESR_EL1, Readable};

use crate::{
    arch::{
        trap::{RawUserContext, TrapFrame},
        TIMER_IRQ_NUM,
    },
    cpu::PrivilegeLevel,
    irq::call_irq_callback_functions,
    user::{ReturnReason, UserContextApi, UserContextApiInternal},
};

/// Userspace CPU context, including general-purpose registers and exception information.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct UserContext {
    user_context: RawUserContext,
    trap: Trap,
    cpu_exception_info: Option<CpuExceptionInfo>,
}

/// General registers.
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[expect(missing_docs)]
pub struct GeneralRegs {
    pub x0: usize,
    pub x1: usize,
    pub x2: usize,
    pub x3: usize,
    pub x4: usize,
    pub x5: usize,
    pub x6: usize,
    pub x7: usize,
    pub x8: usize,  // TP
    pub x9: usize,
    pub x10: usize,
    pub x11: usize,
    pub x12: usize,
    pub x13: usize,
    pub x14: usize,
    pub x15: usize,
    pub x16: usize,
    pub x17: usize,
    pub x18: usize,
    pub x19: usize,
    pub x20: usize,
    pub x21: usize,
    pub x22: usize,
    pub x23: usize,
    pub x24: usize,
    pub x25: usize,
    pub x26: usize,
    pub x27: usize,
    pub x28: usize,
    pub x29: usize // FP
}

/// CPU exception information.
//
// TODO: Refactor the struct into an enum (similar to x86's `CpuException`).
#[expect(missing_docs)]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct CpuExceptionInfo {
    /// The type of the exception.
    pub code: Exception,
    /// The error code associated with the exception.
    pub page_fault_addr: usize,
    pub error_code: usize, // TODO
}

impl Default for UserContext {
    fn default() -> Self {
        UserContext {
            user_context: RawUserContext::default(),
            trap: Trap::Exception(Exception::Unknown),
            cpu_exception_info: None,
        }
    }
}

impl Default for CpuExceptionInfo {
    fn default() -> Self {
        CpuExceptionInfo {
            code: Exception::Unknown,
            page_fault_addr: 0,
            error_code: 0,
        }
    }
}

impl CpuExceptionInfo {
    /// Get corresponding CPU exception
    pub fn cpu_exception(&self) -> CpuException {
        self.code
    }
}

impl UserContext {
    /// Returns a reference to the general registers.
    pub fn general_regs(&self) -> &GeneralRegs {
        &self.user_context.general
    }

    /// Returns a mutable reference to the general registers
    pub fn general_regs_mut(&mut self) -> &mut GeneralRegs {
        &mut self.user_context.general
    }

    /// Returns the trap information.
    pub fn take_exception(&mut self) -> Option<CpuExceptionInfo> {
        self.cpu_exception_info.take()
    }

    /// Sets the thread-local storage pointer.
    pub fn set_tls_pointer(&mut self, tls: usize) {
        self.set_x8(tls)
    }

    /// Gets the thread-local storage pointer.
    pub fn tls_pointer(&self) -> usize {
        self.x8()
    }

    /// Activates the thread-local storage pointer for the current task.
    pub fn activate_tls_pointer(&self) {
        // In RISC-V, `tp` will be loaded at `UserContext::execute`, so it does not need to be
        // activated in advance.
    }
}

impl UserContextApiInternal for UserContext {
    fn execute<F>(&mut self, mut has_kernel_event: F) -> ReturnReason
    where
        F: FnMut() -> bool,
    {
        let esr = ESR_EL1.get();
        let ret = loop {
            self.user_context.run();
            match ESR_EL1::read(ESR_EL1::EC) {
                // Synchronous exception from EL0
                ESR_EL1::EC::SynchronousExceptionLowerEL => {
                    // Handle different types of synchronous exceptions.
                    match ESR_EL1.read(ESR_EL1::ISS) {
                        // Synchronous exception from EL0
                        ESR_EL1::EC::SynchronousExceptionLowerEL => {
                            // Handle different types of synchronous exceptions.
                            match ESR_EL1.read(ESR_EL1::ISS) {
                                // e.g., instruction abort, data abort
                                _ => println!("Unhandled Synchronous Exception from Lower EL"),
                            }
                        },
                        // IRQ from EL0
                        ESR_EL1::EC::IRQLowerEL => {
                            // Handle IRQ. For GIC, this involves reading ICC_IAR1_EL1.
                            /// handle_irq();
                            irq_current(&self.as_trap_frame());
                        },
                        // SVC instruction from EL0
                        ESR_EL1::EC::SVC64 => {
                            // Handle SVC call (system call).
                            /// TODO: handle_syscall(context);
                        },
                        // Data Abort from a lower Exception level
                        ESR_EL1::EC::DataAbortLowerEL => {
                            // Read FAR_EL1 to get the fault address and handle the memory fault.
                            /// TODO: handle_data_abort(context);
                        },
                        // Default case for unknown exceptions
                        _ => {
                            println!("Unrecognized exception type: {:#x}", esr);
                            loop {} // Halt on unknown exception.
                        }

                    }
                }
            }

            if has_kernel_event() {
                break ReturnReason::KernelEvent;
            }
        };

        crate::arch::irq::enable_local();
        ret
    }

    fn as_trap_frame(&self) -> TrapFrame {
        TrapFrame {
            general: self.user_context.general,
            el: self.user_context.el,
            elr_el1: self.user_context.elr_el1,
            spsr_el1: self.user_context.spsr_el1,
            esr_el1: self.user_context.esr_el1,
        }
    }
}

impl UserContextApi for UserContext {
    fn trap_number(&self) -> usize {
        todo!()
    }

    fn trap_error_code(&self) -> usize {
        todo!()
    }

    fn instruction_pointer(&self) -> usize {
        self.user_context.elr_el1
    }

    fn set_instruction_pointer(&mut self, ip: usize) {
        self.user_context.elr_el1 = ip;
    }

    fn stack_pointer(&self) -> usize {
        /// use FP as stack pointer ?
        self.user_context.x29()
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        /// use FP as stack pointer ?
        self.set_x29(sp);
    }
}

macro_rules! cpu_context_impl_getter_setter {
    ( $( [ $field: ident, $setter_name: ident] ),*) => {
        impl UserContext {
            $(
                #[doc = concat!("Gets the value of ", stringify!($field))]
                #[inline(always)]
                pub fn $field(&self) -> usize {
                    self.user_context.general.$field
                }

                #[doc = concat!("Sets the value of ", stringify!($field))]
                #[inline(always)]
                pub fn $setter_name(&mut self, $field: usize) {
                    self.user_context.general.$field = $field;
                }
            )*
        }
    };
}

cpu_context_impl_getter_setter!(
    [x0, set_x0],
    [x1, set_x1],
    [x2, set_x2],
    [x3, set_x3],
    [x4, set_x4],
    [x5, set_x5],
    [x6, set_x6],
    [x7, set_x7],
    [x8, set_x8],
    [x9, set_x9],
    [x10, set_x10],
    [x11, set_x11],
    [x12, set_x12],
    [x13, set_x13],
    [x14, set_x14],
    [x15, set_x15],
    [x16, set_x16],
    [x17, set_x17],
    [x18, set_x18],
    [x19, set_x19],
    [x20, set_x20],
    [x21, set_x21],
    [x22, set_x22],
    [x23, set_x23],
    [x24, set_x24],
    [x25, set_x25],
    [x26, set_x26],
    [x27, set_x27],
    [x28, set_x28],
    [x29, set_x29],
);

/// CPU exception.
pub type CpuException = Exception;

/// The FPU context of user task.
///
/// This could be used for saving both legacy and modern state format.
// FIXME: Implement FPU context on RISC-V platforms.
#[derive(Clone, Debug, Default)]
pub struct FpuContext;

impl FpuContext {
    /// Creates a new FPU context.
    pub fn new() -> Self {
        Self
    }

    /// Saves CPU's current FPU context to this instance, if needed.
    pub fn save(&mut self) {}

    /// Loads CPU's FPU context from this instance, if needed.
    pub fn load(&mut self) {}

    /// Returns the FPU context as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &[]
    }

    /// Returns the FPU context as a mutable byte slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut []
    }
}
