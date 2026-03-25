// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O via PL011 UART.
//!
//! On QEMU's `virt` board the PL011 UART is at physical address 0x0900_0000.
//! We use MMIO to talk to it directly; no initialisation is needed because QEMU
//! already leaves the UART in reset-default state (8N1, TX enabled).

/// PL011 UART base (QEMU virt board).
const PL011_BASE: usize = 0x0900_0000;

/// DR – data register (offset 0x000): write a byte here to transmit.
const PL011_DR: usize = PL011_BASE + 0x000;

/// FR – flag register (offset 0x018): bit 5 = TXFF (TX FIFO full).
const PL011_FR: usize = PL011_BASE + 0x018;

const FR_TXFF: u32 = 1 << 5;

#[inline(always)]
fn read_fr() -> u32 {
    let val: u32;
    // SAFETY: PL011_FR is a valid MMIO address on QEMU virt; the MMU is on and
    // the region is mapped as device memory by the boot page tables.
    unsafe {
        core::ptr::read_volatile(PL011_FR as *const u32)
    }
}

/// Initializes the serial port.
pub(crate) fn init() {
    // Nothing to do: QEMU leaves the PL011 ready for polled TX.
}

/// Sends a byte on the serial port.
pub fn send(data: u8) {
    // Wait until TX FIFO is not full.
    while (read_fr() & FR_TXFF) != 0 {}
    // SAFETY: PL011_DR is a valid MMIO address.
    unsafe {
        core::ptr::write_volatile(PL011_DR as *mut u32, data as u32);
    }
}
