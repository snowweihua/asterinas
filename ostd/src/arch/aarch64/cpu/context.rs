// SPDX-License-Identifier: MPL-2.0

//! CPU execution context control.

use core::{fmt::Debug};

use aarch64_cpu::registers::{FAR_EL1, Readable, TPIDR_EL1, Writeable};

use crate::{
    arch::{
        trap::{RawUserContext, TrapFrame},        
    },
    cpu::PrivilegeLevel,
    irq::call_irq_callback_functions,
    task::scheduler,
    user::{ReturnReason, UserContextApi, UserContextApiInternal},
};

/// Userspace CPU context, including general-purpose registers and exception information.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct UserContext {
    user_context: RawUserContext,
    exception: Option<CpuExceptionInfo>,
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
/// Represents the Exception Class (EC) field [31:26] of the ESR_EL1 register.
/// This field indicates the reason for the exception being taken to EL1.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u32)] // Ensure the enum values match the raw ESR bits
pub enum CpuException {
    /// Unknown reason (e.g., illegal execution state).
    Unknown = 0x00,
    /// Trapped MCR or MRC access.
    TrappedMcrMrc = 0x01,
    /// Trapped LDC or STC access.
    TrappedLdcStc = 0x05,
    /// Trapped access to a System register (MRS or MSR).
    TrappedSysReg = 0x06,
    /// Trapped access to SVE/SIMD/FP functionality.
    TrappedSimdFpSve = 0x07,
    /// Trapped generic timer access.
    TrappedTimer = 0x08,

    /// Instruction Abort from a lower Exception level (EL0).
    InstructionAbortLowerEL = 0x20,
    /// Instruction Abort from the same Exception level (EL1).
    InstructionAbortCurrentEL = 0x21,
    /// PC alignment fault.
    PCAlignmentFault = 0x22,
    /// Data Abort from a lower Exception level (EL0) - often a page fault.
    DataAbortLowerEL = 0x24,
    /// Data Abort from the same Exception level (EL1).
    DataAbortCurrentEL = 0x25,
    /// SP alignment fault.
    SPAlignmentFault = 0x26,
    /// Trapped floating-point exception (AArch64).
    TrappedFPException = 0x2c,

    /// SVC (System Call) instruction execution (AArch64).
    Svc64 = 0x15,

    /// Asynchronous exceptions from the current EL (e.g., IRQ, FIQ).
    AsyncCurrentEL = 0x09,
    /// Asynchronous exceptions from a lower EL (e.g., IRQ, FIQ from EL0).
    AsyncLowerEL = 0x1c, // Base value for IRQ, FIQ, SError from lower EL

    /// A fallback variant for any other EC value not explicitly listed above.
    Other(u8),
}


impl CpuException {
    pub(crate) fn from_esr(esr_el1: usize) -> Self {
        let ec = ((esr_el1 >> 26) & 0x3f) as usize;
        match ec {
            0 => Self::Unknown,
            1 => Self::TrappedMcrMrc,
            5 => Self::TrappedLdcStc,
            6 => Self::TrappedSysReg,
            7 => Self::TrappedSimdFpSve,
            8 => Self::TrappedTimer,
            0x20 => Self::InstructionAbortLowerEL,
            0x21 => Self::InstructionAbortCurrentEL,
            0x22 => Self::PCAlignmentFault,
            0x24 => Self::DataAbortLowerEL,
            0x25 => Self::DataAbortCurrentEL,
            0x26 => Self::SPAlignmentFault,
            0x2c => Self::TrappedFPException,
            0x15 => Self::Svc64,
            0x1c => Self::AsyncLowerEL,
            _ => Self::Other(ec as u8),
        }
    }

    pub(crate) fn new(trap_num: usize, error_code: usize) -> Option<Self> {
        let exception = match trap_num {
            0 => Self::Unknown,
            1 => Self::TrappedMcrMrc,
            5 => Self::TrappedLdcStc,
            6 => Self::TrappedSysReg,
            7 => Self::TrappedSimdFpSve,
            8 => Self::TrappedTimer,
            0x20 => Self::InstructionAbortLowerEL,
            0x21 => Self::InstructionAbortCurrentEL,
            0x22 => Self::PCAlignmentFault,
            0x24 => Self::DataAbortLowerEL,
            0x25 => Self::DataAbortCurrentEL,
            0x26 => Self::SPAlignmentFault,
            0x2c => Self::TrappedFPException,
            0x15 => Self::Svc64,
            0x08 => Self::AsyncCurrentEL,
            0x1c => Self::AsyncLowerEL,

       
            _ => Self::Other(trap_num as u8),
        };

        Some(exception)
    }

    const fn type_(&self) -> CpuExceptionType {
        match self {
            Self::Unknown | Self::PCAlignmentFault | Self::SPAlignmentFault => CpuExceptionType::FaultOrTrap,
            Self::AsyncCurrentEL | Self::AsyncLowerEL | Self::Svc64 => CpuExceptionType::Interrupt,
            Self::TrappedMcrMrc | Self::TrappedLdcStc | Self::TrappedSysReg | Self::TrappedSimdFpSve => CpuExceptionType::Trap,
            Self::InstructionAbortLowerEL | Self::DataAbortLowerEL => CpuExceptionType::Fault,
            Self::InstructionAbortCurrentEL | Self::DataAbortCurrentEL => CpuExceptionType::Abort,

            _ => CpuExceptionType::Fault,
        }
    }

    pub(crate) const fn is_cpu_exception(trap_num: usize) -> bool {
        trap_num <= 0x3f
    }
}

impl Default for UserContext {
    fn default() -> Self {
        UserContext {
            user_context: RawUserContext::default(),
            exception: None,
        }
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
        self.exception.take()
    }

    /// Sets the thread-local storage pointer.
    pub fn set_tls_pointer(&mut self, tls: usize) {
        TPIDR_EL1.set(tls as u64)
    }

    /// Gets the thread-local storage pointer.
    pub fn tls_pointer(&self) -> usize {
        TPIDR_EL1.get() as usize
    }

    /// Activates the thread-local storage pointer for the current task.
    pub fn activate_tls_pointer(&self) {

    }
}

impl UserContextApiInternal for UserContext {
    fn execute<F>(&mut self, mut has_kernel_event: F) -> ReturnReason
    where
        F: FnMut() -> bool,
    {

        // Return when it is syscall or cpu exception type is Fault or Trap.
        let ret = loop {
            scheduler::might_preempt();
            self.user_context.run();
            
            let cpu_exception = CpuException::from_esr(self.user_context.esr_el1);
            match cpu_exception {
                exception if exception.type_().is_fault_or_trap() => {
                    crate::arch::irq::enable_local();
                    // Read FAR_EL1 to capture the faulting virtual address for memory aborts.
                    let far_el1 = FAR_EL1.get() as usize;
                    self.exception = Some(CpuExceptionInfo {
                        code: exception,
                        page_fault_addr: far_el1,
                        error_code: self.user_context.esr_el1,
                    });
                    return ReturnReason::UserException;
                }
                CpuException::Svc64 => {
                    crate::arch::irq::enable_local();
                    return ReturnReason::UserSyscall;
                }
                cpu_exception => {
                    panic!(
                        "cannot handle user CPU exception: {:?}, trapframe: {:?}",
                        cpu_exception,
                        self.as_trap_frame()
                    );
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
            lr: self.user_context.lr,
            elr_el1: self.user_context.elr_el1,
            spsr_el1: self.user_context.spsr_el1,
            esr_el1: self.user_context.esr_el1,
        }
    }
}

/// CPU exception information, compatible with the interface expected by kernel arch modules.
///
/// For AArch64, the exception class (EC) is found in ESR_EL1[31:26], and
/// the faulting address for memory aborts is stored in FAR_EL1.
#[expect(missing_docs)]
#[derive(Clone, Copy, Debug)]
pub struct CpuExceptionInfo {
    /// The AArch64 exception (derived from ESR_EL1).
    pub code: CpuException,
    /// Faulting virtual address (from FAR_EL1 for data/instruction aborts).
    pub page_fault_addr: usize,
    /// Raw ESR_EL1 value (includes ISS bits).
    pub error_code: usize,
}

impl Default for CpuExceptionInfo {
    fn default() -> Self {
        CpuExceptionInfo {
            code: CpuException::Unknown,
            page_fault_addr: 0,
            error_code: 0,
        }
    }
}

impl CpuExceptionInfo {
    /// Returns the CPU exception type.
    pub fn cpu_exception(&self) -> CpuException {
        self.code
    }
}

/// As Osdev Wiki defines(<https://wiki.osdev.org/Exceptions>):
/// CPU exceptions are classified as:
///
/// Faults: These can be corrected and the program may continue as if nothing happened.
///
/// Traps: Traps are reported immediately after the execution of the trapping instruction.
///
/// Aborts: Some severe unrecoverable error.
///
/// But there exists some vector which are special. Vector 1 can be both fault or trap and vector 2 is interrupt.
/// So here we also define FaultOrTrap and Interrupt
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CpuExceptionType {
    /// CPU faults. Faults can be corrected, and the program may continue as if nothing happened.
    Fault,
    /// CPU traps. Traps are reported immediately after the execution of the trapping instruction
    Trap,
    /// Faults or traps
    FaultOrTrap,
    /// CPU interrupts
    Interrupt,
    /// Some severe unrecoverable error
    Abort,
    /// Reserved for future use
    Reserved,
}

impl CpuExceptionType {
    /// Returns whether this exception type is a fault or a trap.
    pub fn is_fault_or_trap(self) -> bool {
        match self {
            CpuExceptionType::Trap | CpuExceptionType::Fault | CpuExceptionType::FaultOrTrap => {
                true
            }
            CpuExceptionType::Abort | CpuExceptionType::Interrupt | CpuExceptionType::Reserved => {
                false
            }
        }
    }
}



impl UserContextApi for UserContext {
    fn trap_number(&self) -> usize {
        (self.user_context.esr_el1 >> 26) & 0x3f
    }

    fn trap_error_code(&self) -> usize {
        self.user_context.esr_el1 & 0x01ff_ffff
    }

    fn instruction_pointer(&self) -> usize {
        self.user_context.elr_el1
    }

    fn set_instruction_pointer(&mut self, ip: usize) {
        self.user_context.elr_el1 = ip;
    }

    fn stack_pointer(&self) -> usize {
        self.user_context.sp_el0
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.user_context.sp_el0 = sp;
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
    [x29, set_x29]
);


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
