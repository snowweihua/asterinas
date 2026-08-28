// SPDX-License-Identifier: MPL-2.0

//! Platform-specific code for the aarch64 platform.

#![expect(dead_code)]

pub mod boot;
pub mod cpu;
pub mod device;
mod gic;
mod io;
pub(crate) mod iommu;
pub(crate) mod irq;
pub(crate) mod mm;
pub(crate) mod ex_table;
pub mod qemu;
pub mod serial;
pub(crate) mod task;
mod timer;
pub mod trap;
pub(crate) mod board;
pub(crate) mod bcm2836_irq;

use aarch64_cpu::registers::*;

pub fn is_rpi3() -> bool {
    board::BoardType::cached() == 2
}

#[cfg(feature = "cvm_guest")]
pub(crate) fn init_cvm_guest() {
    // Unimplemented, no-op
}

pub(crate) unsafe fn late_init_on_bsp() {
    unsafe { trap::init() };
    if crate::arch::board::BoardType::cached() == 2 {
        unsafe { bcm2836_irq::init_on_bsp() };
    } else {
        unsafe { gic::init_on_bsp() };
    }
    let io_mem_builder = io::construct_io_mem_allocator_builder();
    if crate::arch::board::BoardType::cached() == 2 {
        unsafe { crate::boot::smp::boot_all_aps() };
    } else {
        unsafe { crate::boot::smp::boot_all_aps() };
    }
    unsafe { timer::init() };
    if crate::arch::board::BoardType::cached() == 2 {
    } else {
        unsafe { crate::io::init(io_mem_builder) };
    }
}

pub(crate) unsafe fn init_on_ap() {
    bcm2836_irq::init_on_ap();
}

pub(crate) fn interrupts_ack(irq_number: usize) {
    gic::end_interrupt(irq_number);
}

/// Return the frequency of TSC. The unit is Hz.
pub fn tsc_freq() -> u64 {
    let freq = timer::get_timebase_freq();
    if freq != 0 {
        return freq;
    }

    // RPi3 single-core bring-up skips `timer::init()` (it needs a GIC),
    // so `TIMEBASE_FREQ` is never set. Read CNTFRQ_EL0 directly; the
    // firmware initializes it to the system counter frequency.
    let cntfrq: u64;
    unsafe {
        core::arch::asm!(
            "mrs {0}, cntfrq_el0",
            out(reg) cntfrq,
            options(nostack, nomem, preserves_flags)
        );
    }
    cntfrq
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
        core::arch::asm!("isb", options(nostack, nomem, preserves_flags));
    }
}
