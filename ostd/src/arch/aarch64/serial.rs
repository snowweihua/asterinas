// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O.
//!
//! On QEMU the PL011 at 0x0900_0000 is used.  On the Raspberry Pi 3 the
//! firmware and U-Boot bring up the mini-UART (AUX UART1) at GPIO 14/15,
//! which is what the serial capture sees.  Route runtime console output to
//! the mini-UART on RPi3 and keep the PL011 path for QEMU.

const PL011_BASE_PA_QEMU: usize = 0x0900_0000;
const PL011_BASE_PA_RPI3: usize = 0x3F20_1000;
const PL011_LINEAR_OFFSET: usize = 0xffff_8000_0000_0000;

const MINIUART_BASE_PA: usize = 0x3F21_5000;
const MINIUART_IO_OFFSET: usize = 0x40;
const MINIUART_LSR_OFFSET: usize = 0x54;
const MINIUART_LSR_TX_EMPTY: u32 = 1 << 5;
const MINIUART_LSR_RX_READY: u32 = 1 << 0;

const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;
const IM_RXIM: u32 = 1 << 4;
/// PL011 IMSC (Interrupt Mask Set/Clear) is at offset 0x038, not 0x004 (which is RSR/ECR).
const PL011_IMSC_OFFSET: usize = 0x038;

fn is_rpi3() -> bool {
    crate::arch::board::BoardType::cached() == 2
}

fn pl011_base_va() -> usize {
    let base_pa = if is_rpi3() {
        PL011_BASE_PA_RPI3
    } else {
        PL011_BASE_PA_QEMU
    };
    base_pa + PL011_LINEAR_OFFSET
}

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
    unsafe {
        core::ptr::write_volatile((pl011_base_va() + PL011_IMSC_OFFSET) as *mut u32, value)
    }
}

/// RPi3 mini-UART I/O register (data) virtual address.
/// Use the kernel linear mapping so the UART remains accessible after TTBR0 is
/// switched to a user page table (TTBR1 still points to the kernel PT).
#[inline(always)]
fn miniuart_io_va() -> usize {
    MINIUART_BASE_PA + MINIUART_IO_OFFSET + PL011_LINEAR_OFFSET
}

/// RPi3 mini-UART line status register virtual address.
#[inline(always)]
fn miniuart_lsr_va() -> usize {
    MINIUART_BASE_PA + MINIUART_LSR_OFFSET + PL011_LINEAR_OFFSET
}

#[inline(always)]
fn miniuart_read_lsr() -> u32 {
    unsafe { core::ptr::read_volatile(miniuart_lsr_va() as *const u32) }
}

#[inline(always)]
fn miniuart_write(data: u8) {
    unsafe {
        core::ptr::write_volatile(miniuart_io_va() as *mut u32, data as u32);
    }
}

#[inline(always)]
fn miniuart_read() -> u8 {
    unsafe { (core::ptr::read_volatile(miniuart_io_va() as *const u32) & 0xff) as u8 }
}

pub(crate) fn init() {}

pub fn init_rx_irq() {
    if !is_rpi3() {
        set_im(IM_RXIM);
    }
}

pub fn has_data() -> bool {
    if is_rpi3() {
        (miniuart_read_lsr() & MINIUART_LSR_RX_READY) != 0
    } else {
        (read_fr() & FR_RXFE) == 0
    }
}

pub fn receive() -> u8 {
    if is_rpi3() {
        while (miniuart_read_lsr() & MINIUART_LSR_RX_READY) == 0 {}
        miniuart_read()
    } else {
        while (read_fr() & FR_RXFE) != 0 {}
        (read_dr() & 0xff) as u8
    }
}

pub fn send(data: u8) {
    if is_rpi3() {
        while (miniuart_read_lsr() & MINIUART_LSR_TX_EMPTY) == 0 {}
        miniuart_write(data);
    } else {
        while (read_fr() & FR_TXFF) != 0 {}
        unsafe {
            core::ptr::write_volatile((pl011_base_va() + 0x000) as *mut u32, data as u32);
        }
    }
}

pub fn send_direct_pa(data: u8) {
    if is_rpi3() {
        while (unsafe { core::ptr::read_volatile((MINIUART_BASE_PA + MINIUART_LSR_OFFSET) as *const u32) } & MINIUART_LSR_TX_EMPTY) == 0 {}
        unsafe {
            core::ptr::write_volatile((MINIUART_BASE_PA + MINIUART_IO_OFFSET) as *mut u32, data as u32);
        }
    } else {
        while unsafe { core::ptr::read_volatile((PL011_BASE_PA_QEMU + 0x018) as *const u32) } & FR_TXFF != 0 {}
        unsafe {
            core::ptr::write_volatile((PL011_BASE_PA_QEMU + 0x000) as *mut u32, data as u32);
        }
    }
}
