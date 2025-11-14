// SPDX-License-Identifier: MPL-2.0

//! Platform-specific code for the aarch64 platform.

#![expect(dead_code)]

pub mod boot;
pub mod cpu;
pub mod device;
mod io;
pub(crate) mod iommu;
pub(crate) mod irq;
pub(crate) mod mm;
pub mod qemu;
pub(crate) mod serial;
pub(crate) mod task;
mod timer;
pub mod trap;

use core::sync::atomic::Ordering;

use crate::arch::timer::TIMER_IRQ_NUM;

use aarch64_cpu::registers::*;


#[cfg(feature = "cvm_guest")]
pub(crate) fn init_cvm_guest() {
    // Unimplemented, no-op
}

pub(crate) unsafe fn late_init_on_bsp() {
    // SAFETY: This function is called in the boot context of the BSP.
    unsafe { trap::init() };

    let io_mem_builder = io::construct_io_mem_allocator_builder();

    // SAFETY: We're on the BSP and we're ready to boot all APs.
    unsafe { crate::boot::smp::boot_all_aps() };

    // SAFETY: This function is called once and at most once at a proper timing
    // in the boot context of the BSP, with no timer-related operations having
    // been performed.
    unsafe { timer::init() };

    // SAFETY:
    // 1. All the system device memory have been removed from the builder.
    // 2. RISC-V platforms do not have port I/O.
    unsafe { crate::io::init(io_mem_builder) };
}

pub(crate) unsafe fn init_on_ap() {
    unimplemented!()
}

pub(crate) fn interrupts_ack(irq_number: usize) {
    // TODO: We should check for software interrupts too here. Only those external
    // interrupts would go through the IRQ chip.
    if irq_number == TIMER_IRQ_NUM.load(Ordering::Relaxed) as usize {
        return;
    }

    unimplemented!()
}

/// Return the frequency of TSC. The unit is Hz.
pub fn tsc_freq() -> u64 {
    timer::get_timebase_freq()
}

/// Reads the current value of the processor’s time-stamp counter (TSC).
pub fn read_tsc() -> u64 {
    // SAFETY: The CNTVCT_EL0 register can be read at EL0 and EL1.
    // The `get()` method performs the `mrs` instruction safely.
    CNTVCT_EL0.get()
}

/// Reads a hardware generated 64-bit random value.
///
/// Returns None if no random value was generated.
pub fn read_random() -> Option<u64> {
    // FIXME: Implement a hardware random number generator on RISC-V platforms.
    None
}

pub(crate) fn enable_cpu_features() {
    cpu::extension::init();
    // Read the current value and set the FPEN field to enable access.
    // A value of 0b11 means "No trapping of SIMD and FP instructions" for EL0 and EL1.
    // 0b00 means trapped at all levels.
    // We update the register safely.
    unsafe {
        CPACR_EL1.modify(CPACR_EL1::FPEN::TrapNothing); 
    }
}
