// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O.
//!
//! On QEMU the PL011 at 0x0900_0000 is used.  On the Raspberry Pi 3 the
//! PL011 UART at GPIO 14/15 ALT0 is used for serial console.  The mini-UART
//! (AUX UART1) at GPIO 14/15 ALT5 is not used.
//!
//! All runtime MMIO is done through the kernel high-half mapping
//! (crate::mm::kspace::KERNEL_BASE_VADDR) so that the peripheral pages are
//! accessed with Device memory attributes and values are not cached.
//!
//! # Safety
//!
//! UART access is volatile MMIO with Device attributes (see above), so no
//! caching or compiler reordering across the accesses. The TXFF-polled
//! fault-dump path takes no locks and touches no allocator state, which
//! is what makes it safe to call from the synchronous-exception handler.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::mm::kspace::KERNEL_BASE_VADDR;

static FORCE_BLOCKING_SEND: AtomicBool = AtomicBool::new(false);

/// Force `send()` to wait for mini-UART TX space even when local IRQs are
/// disabled. This is used by the panic handler so the full panic message is
/// not truncated.
pub fn set_force_blocking_send(enabled: bool) {
    FORCE_BLOCKING_SEND.store(enabled, Ordering::Relaxed);
}

const PL011_BASE_PA_QEMU: usize = 0x0900_0000;
const PL011_BASE_PA_RPI3: usize = 0x3F20_1000;

const MINIUART_BASE_PA: usize = 0x3F21_5000;
const MINIUART_AUX_ENABLES_OFFSET: usize = 0x04;
const MINIUART_IO_OFFSET: usize = 0x40;
const MINIUART_LSR_OFFSET: usize = 0x54;
const MINIUART_IER_OFFSET: usize = 0x44;
const MINIUART_CNTL_OFFSET: usize = 0x60;
const MINIUART_STAT_OFFSET: usize = 0x64;
const MINIUART_AUX_ENABLES_MINIUART: u32 = 1 << 0;
const MINIUART_LSR_TX_EMPTY: u32 = 1 << 5;
const MINIUART_LSR_RX_READY: u32 = 1 << 0;
// AUX_MU_STAT_REG bit 1: transmitter FIFO can accept at least one more symbol.
const MINIUART_STAT_TX_SPACE: u32 = 1 << 1;
const MINIUART_IER_RX_ENABLE: u32 = 1 << 0;
// The BCM2835 mini-UUART interrupt enable register needs bits 2 and 3 set as
// well as the RX-enable bit; real hardware does not generate interrupts without
// them (FreeBSD uart_dev_mu.c calls this IER_REQUIRED).
const MINIUART_IER_REQUIRED: u32 = 3 << 2;
const MINIUART_IER_RX: u32 = MINIUART_IER_RX_ENABLE | MINIUART_IER_REQUIRED;
const MINIUART_CNTL_RX_ENABLE: u32 = 1 << 0;
const MINIUART_CNTL_TX_ENABLE: u32 = 1 << 1;

/// GPIO block for the RPi3 pinmux.
const GPIO_BASE_PA: usize = 0x3F20_0000;
const GPIO_GPFSEL1_OFFSET: usize = 0x04;
const GPIO_GPPUD_OFFSET: usize = 0x94;
const GPIO_GPPUDCLK0_OFFSET: usize = 0x98;
const GPIO_PIN_14: u32 = 1 << 14;
const GPIO_PIN_15: u32 = 1 << 15;

const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;
const IM_RXIM: u32 = 1 << 4;
/// PL011 IMSC (Interrupt Mask Set/Clear) is at offset 0x038, not 0x004 (which is RSR/ECR).
const PL011_IMSC_OFFSET: usize = 0x038;
/// PL011 ICR (Interrupt Clear Register) is at offset 0x044.
const PL011_ICR_OFFSET: usize = 0x044;
// PL011 line control and control register offsets.
const PL011_IBRD_OFFSET: usize = 0x024;
const PL011_FBRD_OFFSET: usize = 0x028;
const PL011_LCR_H_OFFSET: usize = 0x02C;
const PL011_CR_OFFSET: usize = 0x030;
const PL011_CR_UARTEN: u32 = 1 << 0;
const PL011_CR_TXE: u32 = 1 << 8;
const PL011_CR_RXE: u32 = 1 << 9;
const PL011_LCR_H_WLEN_8: u32 = 3 << 5;
const PL011_LCR_H_FEN: u32 = 1 << 4;

static PL011_INITIALIZED: AtomicBool = AtomicBool::new(false);
static PL011_INIT_DONE: AtomicBool = AtomicBool::new(false);

fn pl011_gpio_base_va() -> usize {
    GPIO_BASE_PA + KERNEL_BASE_VADDR
}

fn pl011_gpio_init() {
    let gpio_base = pl011_gpio_base_va();
    let gpfsel1 = (gpio_base + GPIO_GPFSEL1_OFFSET) as *mut u32;
    let mut val = unsafe { core::ptr::read_volatile(gpfsel1) };
    val &= !((7 << 12) | (7 << 15));
    val |= (4 << 12) | (4 << 15);
    unsafe { core::ptr::write_volatile(gpfsel1, val) };

    unsafe { core::ptr::write_volatile((gpio_base + GPIO_GPPUD_OFFSET) as *mut u32, 0) };
    for _ in 0..150 {
        unsafe { core::arch::asm!("nop", options(nomem, nostack, preserves_flags)) };
    }
    unsafe {
        core::ptr::write_volatile(
            (gpio_base + GPIO_GPPUDCLK0_OFFSET) as *mut u32,
            GPIO_PIN_14 | GPIO_PIN_15,
        )
    };
    for _ in 0..150 {
        unsafe { core::arch::asm!("nop", options(nomem, nostack, preserves_flags)) };
    }
    unsafe { core::ptr::write_volatile((gpio_base + GPIO_GPPUD_OFFSET) as *mut u32, 0) };
    unsafe { core::ptr::write_volatile((gpio_base + GPIO_GPPUDCLK0_OFFSET) as *mut u32, 0) };
}

fn pl011_init() {
    if PL011_INIT_DONE.load(Ordering::Relaxed) {
        return;
    }
    PL011_INIT_DONE.store(true, Ordering::Relaxed);

    pl011_gpio_init();

    unsafe {
        let aux_base = miniuart_base_va();
        let aux_en = core::ptr::read_volatile(
            (aux_base + MINIUART_AUX_ENABLES_OFFSET) as *const u32,
        );
        core::ptr::write_volatile(
            (aux_base + MINIUART_AUX_ENABLES_OFFSET) as *mut u32,
            aux_en & !MINIUART_AUX_ENABLES_MINIUART,
        );
        for _ in 0..150 {
            core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
        }
        core::ptr::write_volatile(
            (aux_base + MINIUART_AUX_ENABLES_OFFSET) as *mut u32,
            aux_en,
        );
    }

    pl011_ensure_init();

    unsafe {
        let base = pl011_base_va();
        core::ptr::write_volatile(
            (base + PL011_ICR_OFFSET) as *mut u32,
            0x7FF,
        );
        set_im(IM_RXIM);
        crate::arch::bcm2836_irq::enable_uart_irq();
    }
    core::sync::atomic::fence(Ordering::SeqCst);
}

fn is_rpi3() -> bool {
    crate::arch::is_rpi3()
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

/// Ensure the QEMU PL011 is enabled before any TX/RX.
///
/// QEMU's PL011 model is present at boot but does not echo DR writes unless the
/// UART has been enabled, so early panics would otherwise produce no output.
fn pl011_ensure_init() {
    if PL011_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    PL011_INITIALIZED.store(true, Ordering::Relaxed);

    unsafe {
        let base = pl011_base_va();
        // Disable the UART before configuring it.
        core::ptr::write_volatile((base + PL011_CR_OFFSET) as *mut u32, 0);
        // Divisor for ~115200 baud.
        core::ptr::write_volatile((base + PL011_IBRD_OFFSET) as *mut u32, 26);
        core::ptr::write_volatile((base + PL011_FBRD_OFFSET) as *mut u32, 3);
        // 8N1 with FIFOs enabled.
        core::ptr::write_volatile(
            (base + PL011_LCR_H_OFFSET) as *mut u32,
            PL011_LCR_H_WLEN_8 | PL011_LCR_H_FEN,
        );
        // Enable UART, transmitter, and receiver.
        core::ptr::write_volatile(
            (base + PL011_CR_OFFSET) as *mut u32,
            PL011_CR_UARTEN | PL011_CR_TXE | PL011_CR_RXE,
        );
        // Ensure UART configuration is visible before returning.
        // Without this barrier, QEMU's PL011 may not have processed the enable.
        core::sync::atomic::fence(Ordering::SeqCst);
    }
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

/// RPi3 mini-UART status register virtual address.
#[inline(always)]
fn miniuart_stat_va() -> usize {
    MINIUART_BASE_PA + MINIUART_STAT_OFFSET + KERNEL_BASE_VADDR
}

/// RPi3 mini-UART base virtual address.
#[inline(always)]
fn miniuart_base_va() -> usize {
    MINIUART_BASE_PA + KERNEL_BASE_VADDR
}

/// RPi3 GPIO block base virtual address.
#[inline(always)]
fn miniuart_gpio_base_va() -> usize {
    GPIO_BASE_PA + KERNEL_BASE_VADDR
}

#[inline(always)]
fn miniuart_read_lsr() -> u32 {
    unsafe { core::ptr::read_volatile(miniuart_lsr_va() as *const u32) }
}

#[inline(always)]
fn miniuart_read_stat() -> u32 {
    unsafe { core::ptr::read_volatile(miniuart_stat_va() as *const u32) }
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

pub(crate) fn init() {
    if is_rpi3() {
        pl011_init();
    }
}

/// Returns the hardware IRQ number used by the runtime serial console.
///
/// - RPi3: GPU IRQ 57 (PL011 UART).
/// - QEMU `virt`: SPI 33 (PL011).
pub fn irq_num() -> u8 {
    if is_rpi3() {
        crate::arch::bcm2836_irq::UART_IRQ_NUM as u8
    } else {
        33
    }
}

pub fn init_rx_irq() {
    if is_rpi3() {
        set_im(IM_RXIM);
    } else {
        set_im(IM_RXIM);
    }
}


pub fn reenable_rx_irq() {
    if is_rpi3() {
        crate::arch::bcm2836_irq::enable_uart_irq();
    }
}

pub fn has_data() -> bool {
    if is_rpi3() {
        (read_fr() & FR_RXFE) == 0
    } else {
        (read_fr() & FR_RXFE) == 0
    }
}

pub fn receive() -> u8 {
    if is_rpi3() {
        while read_fr() & FR_RXFE != 0 {}
        (read_dr() & 0xff) as u8
    } else {
        while read_fr() & FR_RXFE != 0 {}
        (read_dr() & 0xff) as u8
    }
}

pub fn send(data: u8) {
    if is_rpi3() {
        pl011_ensure_init();
        while read_fr() & FR_TXFF != 0 {}
        unsafe {
            core::ptr::write_volatile((pl011_base_va() + 0x000) as *mut u32, data as u32);
        }
    } else {
        pl011_ensure_init();
        while read_fr() & FR_TXFF != 0 {}
        unsafe {
            core::ptr::write_volatile((pl011_base_va() + 0x000) as *mut u32, data as u32);
        }
    }
}

/// Send multiple bytes with local IRQs disabled to ensure proper UART synchronization.
///
/// This is used to "prime" the UART after initialization to ensure QEMU's PL011
/// emulation properly processes TX/RX state transitions.
pub fn send_burst_with_irq_disabled(data: &[u8]) {
    let irq_guard = crate::irq::disable_local();
    for &byte in data {
        send(byte);
    }
    drop(irq_guard);
}

