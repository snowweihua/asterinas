// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O via PL011 UART.

const PL011_BASE_PA_QEMU: usize = 0x0900_0000;
const PL011_BASE_PA_RPI3: usize = 0x3F215030;
const PL011_LINEAR_OFFSET: usize = 0xffff_8000_0000_0000;

fn pl011_base_va() -> usize {
    let cached = crate::arch::board::BoardType::cached();
    let base_pa = if cached == 2 {
        PL011_BASE_PA_RPI3
    } else {
        PL011_BASE_PA_QEMU
    };
    base_pa + PL011_LINEAR_OFFSET
}

const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;
const IM_RXIM: u32 = 1 << 4;

#[inline(always)]
fn read_fr() -> u32 {
    unsafe { core::ptr::read_volatile((pl011_base_va() + 0x018) as *const u32) }
}

#[inline(always)]
fn read_dr() -> u32 {
    unsafe { core::ptr::read_volatile((pl011_base_va() + 0x000) as *const u32) }
}

#[inline(always)]
fn set_im(value: u32) {
    unsafe { core::ptr::write_volatile((pl011_base_va() + 0x004) as *mut u32, value) }
}

pub(crate) fn init() {}

pub fn init_rx_irq() {
    set_im(IM_RXIM);
}

pub fn has_data() -> bool {
    (read_fr() & FR_RXFE) == 0
}

pub fn receive() -> u8 {
    while (read_fr() & FR_RXFE) != 0 {}
    (read_dr() & 0xff) as u8
}

pub fn send(data: u8) {
    while (read_fr() & FR_TXFF) != 0 {}
    unsafe {
        core::ptr::write_volatile((pl011_base_va() + 0x000) as *mut u32, data as u32);
    }
}
