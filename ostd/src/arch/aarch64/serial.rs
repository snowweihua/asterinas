// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O.
//!
//! On QEMU the PL011 at 0x0900_0000 is used.  On the Raspberry Pi 3 the
//! firmware and U-Boot bring up the mini-UART (AUX UART1) at GPIO 14/15,
//! which is what the serial capture sees.  Route runtime console output to
//! the mini-UART on RPi3 and keep the PL011 path for QEMU.
//!
//! All runtime MMIO is done through the kernel high-half mapping
//! (crate::mm::kspace::KERNEL_BASE_VADDR) so that the peripheral pages are
//! accessed with Device memory attributes and values are not cached.

use crate::mm::kspace::KERNEL_BASE_VADDR;

const PL011_BASE_PA_QEMU: usize = 0x0900_0000;
const PL011_BASE_PA_RPI3: usize = 0x3F20_1000;

const MINIUART_BASE_PA: usize = 0x3F21_5000;
const MINIUART_AUX_ENABLES_OFFSET: usize = 0x04;
const MINIUART_IO_OFFSET: usize = 0x40;
const MINIUART_LSR_OFFSET: usize = 0x54;
const MINIUART_IER_OFFSET: usize = 0x44;
const MINIUART_CNTL_OFFSET: usize = 0x60;
const MINIUART_AUX_ENABLES_MINIUART: u32 = 1 << 0;
const MINIUART_LSR_TX_EMPTY: u32 = 1 << 5;
const MINIUART_LSR_RX_READY: u32 = 1 << 0;
const MINIUART_IER_RX_ENABLE: u32 = 1 << 0;
// The BCM2835 mini-UUART interrupt enable register needs bits 2 and 3 set as
// well as the RX-enable bit; real hardware does not generate interrupts without
// them (FreeBSD uart_dev_mu.c calls this IER_REQUIRED).
const MINIUART_IER_REQUIRED: u32 = 3 << 2;
const MINIUART_IER_RX: u32 = MINIUART_IER_RX_ENABLE | MINIUART_IER_REQUIRED;
const MINIUART_CNTL_RX_ENABLE: u32 = 1 << 0;
const MINIUART_CNTL_TX_ENABLE: u32 = 1 << 1;

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
    base_pa + KERNEL_BASE_VADDR
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
#[inline(always)]
fn miniuart_io_va() -> usize {
    MINIUART_BASE_PA + MINIUART_IO_OFFSET + KERNEL_BASE_VADDR
}

/// RPi3 mini-UART line status register virtual address.
#[inline(always)]
fn miniuart_lsr_va() -> usize {
    MINIUART_BASE_PA + MINIUART_LSR_OFFSET + KERNEL_BASE_VADDR
}

/// RPi3 mini-UART base virtual address.
#[inline(always)]
fn miniuart_base_va() -> usize {
    MINIUART_BASE_PA + KERNEL_BASE_VADDR
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

/// Returns the hardware IRQ number used by the runtime serial console.
///
/// - RPi3: GPU IRQ 29 (AUX mini-UART).
/// - QEMU `virt`: SPI 33 (PL011).
pub fn irq_num() -> u8 {
    if is_rpi3() {
        crate::arch::bcm2836_irq::MINIUART_IRQ_NUM as u8
    } else {
        33
    }
}

pub fn init_rx_irq() {
    if is_rpi3() {
        unsafe {
            // Enable the mini-UART RX interrupt.  Preserve the line settings
            // (LCR, MCR, BAUD) that the firmware/U-Boot already configured for
            // 115200 8N1; re-writing the baud register is unsafe because the AUX
            // clock may not be exactly 250 MHz.
            let base = miniuart_base_va();

            let aux_en =
                core::ptr::read_volatile((base + MINIUART_AUX_ENABLES_OFFSET) as *const u32);
            core::ptr::write_volatile(
                (base + MINIUART_AUX_ENABLES_OFFSET) as *mut u32,
                aux_en | MINIUART_AUX_ENABLES_MINIUART,
            );

            let cntl = core::ptr::read_volatile((base + MINIUART_CNTL_OFFSET) as *const u32);
            core::ptr::write_volatile(
                (base + MINIUART_CNTL_OFFSET) as *mut u32,
                cntl | MINIUART_CNTL_RX_ENABLE | MINIUART_CNTL_TX_ENABLE,
            );

            // Clear any stale mini-UART interrupt/FIFO state before enabling.
            const MINIUART_IIR_OFFSET: usize = 0x48;
            core::ptr::write_volatile(
                (base + MINIUART_IIR_OFFSET) as *mut u32,
                0x06, // clear receive and transmit FIFOs (FreeBSD IIR_CLEAR)
            );

            // Enable the RX interrupt.  The BCM2835 mini-UART needs bits 2 and 3
            // of IER set as well as bit 0 to generate interrupts on real hardware.
            core::ptr::write_volatile(
                (base + MINIUART_IER_OFFSET) as *mut u32,
                MINIUART_IER_RX,
            );

            let aux_en2 =
                core::ptr::read_volatile((base + MINIUART_AUX_ENABLES_OFFSET) as *const u32);
            let cntl2 = core::ptr::read_volatile((base + MINIUART_CNTL_OFFSET) as *const u32);
            let ier = core::ptr::read_volatile((base + MINIUART_IER_OFFSET) as *const u32);
            crate::console::early_print(format_args!(
                "[init_rx_irq] base={:#x} aux_en={:#x} cntl={:#x} ier={:#x}\n",
                base, aux_en2, cntl2, ier
            ));
        }
    } else {
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
