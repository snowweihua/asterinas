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

/// DR – data register (offset 0x000): write a byte here to transmit, read to receive.
const PL011_DR: usize = PL011_BASE_VA + 0x000;

/// FR – flag register (offset 0x018): bit 5 = TXFF, bit 4 = RXFE.
const PL011_FR: usize = PL011_BASE_VA + 0x018;

/// IER – interrupt enable register (offset 0x004): bit 4 = RXIM.
const PL011_IM: usize = PL011_BASE_VA + 0x004;

const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;
const IM_RXIM: u32 = 1 << 4;

#[inline(always)]
fn read_fr() -> u32 {
    // SAFETY: PL011_FR is a valid MMIO address in the kernel linear map.
    unsafe { core::ptr::read_volatile(PL011_FR as *const u32) }
}

#[inline(always)]
fn read_dr() -> u32 {
    // SAFETY: PL011_DR is a valid MMIO address in the kernel linear map.
    unsafe { core::ptr::read_volatile(PL011_DR as *const u32) }
}

#[inline(always)]
fn set_im(value: u32) {
    // SAFETY: PL011_IM is a valid MMIO address in the kernel linear map.
    unsafe { core::ptr::write_volatile(PL011_IM as *mut u32, value) }
}

/// Initializes the serial port.
pub(crate) fn init() {}

/// Initializes the serial port for RX with interrupt support.
pub fn init_rx_irq() {
    set_im(IM_RXIM);
}

/// Returns true if there is data available to read.
pub fn has_data() -> bool {
    (read_fr() & FR_RXFE) == 0
}

/// Reads a byte from the UART (blocking until data is available).
pub fn receive() -> u8 {
    while (read_fr() & FR_RXFE) != 0 {}
    (read_dr() & 0xff) as u8
}

/// Sends a byte on the serial port.
pub fn send(data: u8) {
    while (read_fr() & FR_TXFF) != 0 {}
    // SAFETY: PL011_DR is a valid MMIO address in the kernel linear map.
    unsafe {
        core::ptr::write_volatile(PL011_DR as *mut u32, data as u32);
    }
}
