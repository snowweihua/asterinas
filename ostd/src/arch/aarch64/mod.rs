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
pub mod qemu;
pub mod serial;
pub(crate) mod task;
mod timer;
pub mod trap;
pub(crate) mod board;

use aarch64_cpu::registers::*;

/// Write a single byte to the PL011 UART via linear map (for low-level probing).
/// Safe to call with IRQs disabled. Does NOT use any locks.
/// The address is the known, pre-validated PL011 UART MMIO address.
#[inline(always)]
pub fn uart_probe(c: u8) {
    let uart_va: usize = 0xffff_8000_0900_0000;
    unsafe {
        core::arch::asm!(
            "str {w}, [{addr}]",
            w = in(reg) c as u32,
            addr = in(reg) uart_va,
            options(nostack, nomem)
        );
    }
}

#[cfg(feature = "cvm_guest")]
pub(crate) fn init_cvm_guest() {
    // Unimplemented, no-op
}

pub(crate) unsafe fn late_init_on_bsp() {
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[late.0] start\n"); }
    unsafe { trap::init() };
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[late.trap] after trap::init\n"); }
    unsafe { gic::init_on_bsp() };
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[late.gic] after gic::init_on_bsp\n"); }
    unsafe { crate::arch::boot::pl011_puts(b"[late.io_mem.0] before construct_io_mem_allocator_builder\n"); }
    let io_mem_builder = io::construct_io_mem_allocator_builder();
    unsafe { crate::arch::boot::pl011_puts(b"[late.io_mem.1] after construct_io_mem_allocator_builder\n"); }
    if crate::arch::board::BoardType::cached() == 2 {
        unsafe { crate::arch::boot::pl011_puts(b"[late.smp.skip] RPi3 single-core\n"); }
    } else {
        unsafe { crate::arch::boot::pl011_puts(b"[late.smp.0] before boot_all_aps\n"); }
        unsafe { crate::boot::smp::boot_all_aps() };
        #[cfg(target_arch = "aarch64")]
        unsafe { crate::arch::boot::pl011_puts(b"[late.smp] after boot_all_aps\n"); }
    }
    if crate::arch::board::BoardType::cached() == 2 {
        unsafe { crate::arch::boot::pl011_puts(b"[late.timer.skip] RPi3\n"); }
    } else {
        unsafe { crate::arch::boot::pl011_puts(b"[late.timer.0] before timer::init\n"); }
        unsafe { timer::init() };
        #[cfg(target_arch = "aarch64")]
        unsafe { crate::arch::boot::pl011_puts(b"[late.timer] after timer::init\n"); }
    }
    if crate::arch::board::BoardType::cached() == 2 {
        unsafe { crate::arch::boot::pl011_puts(b"[late.io.skip] RPi3\n"); }
    } else {
        unsafe { crate::arch::boot::pl011_puts(b"[late.io.0] before io::init\n"); }
        unsafe { crate::io::init(io_mem_builder) };
        #[cfg(target_arch = "aarch64")]
        unsafe { crate::arch::boot::pl011_puts(b"[late.io] after io::init\n"); }
    }
}

pub(crate) unsafe fn init_on_ap() {
    unimplemented!()
}

pub(crate) fn interrupts_ack(irq_number: usize) {
    gic::end_interrupt(irq_number);
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
        core::arch::asm!("isb", options(nostack, nomem, preserves_flags));
    }
}
