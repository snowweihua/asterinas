// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O via PL011 UART.
//!
//! On QEMU's `virt` board the PL011 UART is at physical address 0x0900_0000.
//! We use MMIO to talk to it directly via the kernel linear map; no initialisation
//! is needed because QEMU already leaves the UART in reset-default state (8N1, TX enabled).

/// PL011 UART base PA (QEMU virt board).
const PL011_BASE_PA: usize = 0x0900_0000;

/// PL011 UART base VA in the kernel linear map.
/// On AArch64, the linear map maps PA -> VA as: VA = PA + 0xffff_8000_0000_0000
const PL011_BASE_VA: usize = 0xffff_8000_0900_0000;

/// DR – data register (offset 0x000): write a byte here to transmit.
const PL011_DR: usize = PL011_BASE_VA + 0x000;

/// FR – flag register (offset 0x018): bit 5 = TXFF (TX FIFO full).
const PL011_FR: usize = PL011_BASE_VA + 0x018;

const FR_TXFF: u32 = 1 << 5;

#[inline(always)]
fn read_fr() -> u32 {
    // SAFETY: PL011_FR is a valid MMIO address in the kernel linear map.
    // The boot page tables map this device memory region.
    unsafe { core::ptr::read_volatile(PL011_FR as *const u32) }
}

/// Initializes the serial port.
pub(crate) fn init() {
    // Nothing to do: QEMU leaves the PL011 ready for polled TX.
}

/// Sends a byte on the serial port.
pub fn send(data: u8) {
    // Wait until TX FIFO is not full.
    while (read_fr() & FR_TXFF) != 0 {}
    // SAFETY: PL011_DR is a valid MMIO address in the kernel linear map.
    unsafe {
        core::ptr::write_volatile(PL011_DR as *mut u32, data as u32);
    }
}
