// SPDX-License-Identifier: MPL-2.0

//! Interrupts.
use core::arch::asm;

use super::gic;
use crate::cpu::PinCurrentCpu;

pub(crate) const IRQ_NUM_MIN: u8 = 0;
pub(crate) const IRQ_NUM_MAX: u8 = 255;


pub(crate) struct IrqRemapping {
    _private: (),
}

impl IrqRemapping {
    pub(crate) const fn new() -> Self {
        Self { _private: () }
    }

    /// Initializes the remapping entry for the specific IRQ number.
    ///
    /// This will do nothing if the entry is already initialized or interrupt
    /// remapping is disabled or not supported by the architecture.
    pub(crate) fn init(&self, irq_num: u8) {
        gic::init_interrupt(irq_num);
    }

    /// Gets the remapping index of the IRQ line.
    ///
    /// This method will return `None` if interrupt remapping is disabled or
    /// not supported by the architecture.
    pub(crate) fn remapping_index(&self) -> Option<u16> {
        None
    }
}


// FIXME: Mark this as unsafe. See
// <https://github.com/asterinas/asterinas/issues/1120#issuecomment-2748696592>.
pub fn enable_local() {
    unsafe {
        core::arch::asm!(
            "mov x9, x30",
            "msr DAIFClr, #0b0011",
            "isb",
            "mov x30, x9",
            out("x9") _,
            options(nostack, nomem),
        );
    }
}

/// Enables local IRQs and halts the CPU to wait for interrupts.
///
/// This method guarantees that no interrupts can occur in the middle. In other words, IRQs must
/// either have been processed before this method is called, or they must wake the CPU up from the
/// halting state.
//
// FIXME: Mark this as unsafe. See
// <https://github.com/asterinas/asterinas/issues/1120#issuecomment-2748696592>.
pub(crate) fn enable_local_and_halt() {
    // Enable interrupts by clearing the I and F bits in PSTATE.
    // The `daifclr` instruction clears the specified bits (D, A, I, F) in the DAIF register.
    // `0b0011` corresponds to clearing the I (IRQ) and F (FIQ) bits.
    // The `nomem` option tells the compiler this instruction has no memory side effects.
    unsafe {
        asm!(
            "msr DAIFClr, #0b0011",
            "isb",
            options(nomem, nostack)
        );
    }

    // Enter a low-power, idle state and wait for an interrupt.
    // The `wfi` instruction pauses execution until an event occurs (e.g., an interrupt).
    // The `nomem` and `nostack` options are also applicable here.
    unsafe {
        asm!("wfi", options(nomem, nostack));
    }
}

pub(crate) fn disable_local() {
    unsafe {
        core::arch::asm!(
            "mov x9, x30",
            "msr DAIFSet, #0b0011",
            "mov x30, x9",
            out("x9") _,
            options(nostack, nomem),
        );
    }
}

pub(crate) fn is_local_enabled() -> bool {
    let daif: u64;
    // Read the DAIF register into a local variable.
    // The `mrs` instruction moves the value from the system register to a general-purpose register.
    // The `DAIF` register contains the interrupt mask bits.
    // The `nomem` and `nostack` options are used to indicate that the instruction has no memory side effects.
    unsafe {
        asm!(
            "mrs {}, DAIF",
            out(reg) daif,
            options(nomem, nostack)
        );
    }

    // The I (IRQ) and F (FIQ) mask bits are at positions 7 and 6, respectively, in the DAIF register.
    // If a mask bit is 1, the corresponding interrupt is disabled.
    // Therefore, if both are 0, interrupts are enabled.
    // `(daif >> 6) & 0b11` extracts the I and F bits.
    ((daif >> 6) & 0b11) == 0
}

// ####### Inter-Processor Interrupts (IPIs) #######

/// Hardware-specific, architecture-dependent CPU ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HwCpuId(u32);

impl HwCpuId {
    pub(crate) fn read_current(_guard: &dyn PinCurrentCpu) -> Self {
        let mpidr: u64;
        unsafe {
            core::arch::asm!("mrs {0}, mpidr_el1", out(reg) mpidr, options(nostack, nomem, preserves_flags));
        }
        Self((mpidr & 0x3) as u32)
    }
}

/// Sends a general inter-processor interrupt (IPI) to the specified CPU.
///
/// # Safety
///
/// The caller must ensure that the interrupt number is valid and that
/// the corresponding handler is configured correctly on the remote CPU.
/// Furthermore, invoking the interrupt handler must also be safe.
pub(crate) unsafe fn send_ipi(_hw_cpu_id: HwCpuId, _irq_num: u8, _guard: &dyn PinCurrentCpu) {
    unimplemented!()
}
