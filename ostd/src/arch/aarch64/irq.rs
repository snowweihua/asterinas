// SPDX-License-Identifier: MPL-2.0

//! Interrupts.
use core::arch::asm;

use spin::Once;

use super::gic;
use crate::{cpu::PinCurrentCpu, irq::IrqLine, prelude::Result};

/// SGI number used for IPIs on GIC systems (QEMU `virt`).
///
/// SGIs 0-15 are unused by OSTD on AArch64; the timer uses PPIs and the
/// UART uses an SPI.
const SGI_IPI_NUM: u8 = 1;

/// The allocated IPI line. Kept alive for the lifetime of the kernel so the
/// inter-processor-call callback is never unregistered.
static IPI_LINE: Once<IrqLine> = Once::new();

/// Initializes IPI state on the BSP: registers the handler, then unmasks
/// the doorbell on this core. APs boot after this, so their doorbells are
/// always handled.
///
/// # Safety
///
/// Must run on the BSP before any AP can send an IPI to this core.
pub(in crate::arch) unsafe fn init_ipi_on_bsp() {
    let irq_num = ipi_irq_num();
    let mut line =
        IrqLine::alloc_specific(irq_num).expect("IPI IRQ line is already taken");
    // SAFETY: The queued function runs in IRQ context on the target core,
    // which is exactly what `do_inter_processor_call` requires.
    line.on_active(|trap_frame| unsafe {
        crate::smp::do_inter_processor_call(trap_frame)
    });
    IPI_LINE.call_once(|| line);
    enable_ipi_on_current_core();
}

/// Unmasks the IPI doorbell on an AP.
///
/// # Safety
///
/// Must run before other cores can send IPIs to this core.
pub(in crate::arch) unsafe fn init_ipi_on_ap() {
    enable_ipi_on_current_core();
}

fn is_rpi3() -> bool {
    crate::arch::board::BoardType::cached() == 2
}

fn ipi_irq_num() -> u8 {
    if is_rpi3() {
        crate::arch::bcm2836_irq::IPI_IRQ_NUM as u8
    } else {
        SGI_IPI_NUM
    }
}

fn enable_ipi_on_current_core() {
    if is_rpi3() {
        crate::arch::bcm2836_irq::enable_ipi_irq();
    } else {
        super::gic::init_interrupt(SGI_IPI_NUM);
    }
}

pub(crate) const IRQ_NUM_MIN: u8 = 0;
pub(crate) const IRQ_NUM_MAX: u8 = 255;

pub(crate) struct HwIrqLine {
    irq_num: u8,
}

impl HwIrqLine {
    pub(crate) fn new(irq_num: u8) -> Self {
        Self { irq_num }
    }

    pub(crate) fn irq_num(&self) -> u8 {
        self.irq_num
    }

    pub(crate) fn ack(&self) {
        super::gic::end_interrupt(self.irq_num as usize);
    }
}


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

pub(crate) fn disable_local_and_halt() -> ! {
    disable_local();
    loop {
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
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
/// On the Raspberry Pi 3 (no GIC) this rings the QA7 mailbox-0 doorbell of
/// the target core; on GIC systems it sends an SGI.
///
/// # Safety
///
/// The caller must ensure that the interrupt number is valid and that
/// the corresponding handler is configured correctly on the remote CPU.
/// Furthermore, invoking the interrupt handler must also be safe.
pub(crate) fn send_ipi(hw_cpu_id: HwCpuId, _guard: &dyn PinCurrentCpu) -> Result<()> {
    if is_rpi3() {
        crate::arch::bcm2836_irq::send_ipi_to_core(hw_cpu_id.0);
        Ok(())
    } else {
        let cpu = hw_cpu_id.0;
        if cpu >= 8 {
            return Err(crate::error::Error::InvalidArgs);
        }
        super::gic::send_sgi(SGI_IPI_NUM, 1 << cpu);
        Ok(())
    }
}
