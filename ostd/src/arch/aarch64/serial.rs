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
            const MINIUART_IIR_OFFSET: usize = 0x48;
            const MINIUART_LCR_OFFSET: usize = 0x4C;
            const MINIUART_MCR_OFFSET: usize = 0x50;
            const MINIUART_BAUD_OFFSET: usize = 0x68;

            let base = miniuart_base_va();

            // Preserve the baud rate U-Boot chose; reset modem control to a
            // known value (RTS high, no flow control).
            let aux_en_before =
                core::ptr::read_volatile((base + MINIUART_AUX_ENABLES_OFFSET) as *const u32);
            let baud_before =
                core::ptr::read_volatile((base + MINIUART_BAUD_OFFSET) as *const u32) & 0xffff;

            // Re-initialise the mini-UART from a known-good sequence. Disable
            // RX/TX while configuring so the IER/FIFO setup is not raced by
            // incoming data.
            core::ptr::write_volatile(
                (base + MINIUART_AUX_ENABLES_OFFSET) as *mut u32,
                aux_en_before | MINIUART_AUX_ENABLES_MINIUART,
            );
            core::ptr::write_volatile((base + MINIUART_CNTL_OFFSET) as *mut u32, 0);

            // 8-bit mode and clear DLAB so IER is the interrupt enable register.
            core::ptr::write_volatile((base + MINIUART_LCR_OFFSET) as *mut u32, 3);
            // RTS high, no auto flow control.
            core::ptr::write_volatile((base + MINIUART_MCR_OFFSET) as *mut u32, 0);
            core::ptr::write_volatile((base + MINIUART_BAUD_OFFSET) as *mut u32, baud_before);

            // Clear the FIFOs and any pending interrupt state.
            core::ptr::write_volatile(
                (base + MINIUART_IIR_OFFSET) as *mut u32,
                0xC6, // clear receive and transmit FIFOs, keep FIFO enable bits
            );

            // Enable RX interrupts so input is driven by the AUX IRQ path.
            // The AUX enable bit in the peripheral controller is set separately
            // by the kernel driver; the timer tick still re-enables it as a
            // fallback if the firmware clears it.
            core::ptr::write_volatile((base + MINIUART_IER_OFFSET) as *mut u32, MINIUART_IER_RX);

            // Re-route GPIO 14/15 to mini-UART (alt5) and disable pull-up/down.
            // The firmware or U-Boot may leave these pins configured for a
            // different function, which can make RX input appear dead even
            // though TX output works.
            let gpio_base = miniuart_gpio_base_va();
            let gpfsel1 = (gpio_base + GPIO_GPFSEL1_OFFSET) as *mut u32;
            let mut gpfsel1_val = core::ptr::read_volatile(gpfsel1);
            gpfsel1_val &= !((7 << 12) | (7 << 15));
            gpfsel1_val |= (2 << 12) | (2 << 15);
            core::ptr::write_volatile(gpfsel1, gpfsel1_val);

            core::ptr::write_volatile(
                (gpio_base + GPIO_GPPUD_OFFSET) as *mut u32,
                0,
            );
            for _ in 0..150 {
                core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
            }
            core::ptr::write_volatile(
                (gpio_base + GPIO_GPPUDCLK0_OFFSET) as *mut u32,
                GPIO_PIN_14 | GPIO_PIN_15,
            );
            for _ in 0..150 {
                core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
            }
            core::ptr::write_volatile((gpio_base + GPIO_GPPUD_OFFSET) as *mut u32, 0);
            core::ptr::write_volatile((gpio_base + GPIO_GPPUDCLK0_OFFSET) as *mut u32, 0);

            // Re-enable RX and TX.
            core::ptr::write_volatile(
                (base + MINIUART_CNTL_OFFSET) as *mut u32,
                MINIUART_CNTL_RX_ENABLE | MINIUART_CNTL_TX_ENABLE,
            );
        }
    } else {
        set_im(IM_RXIM);
    }
}

/// Re-enable the mini-UART RX interrupt at the peripheral interrupt controller.
///
/// The VideoCore firmware can overwrite ENABLE_IRQS_1 while managing its own
/// interrupts, so the AUX enable bit may need to be set again after init.
pub fn reenable_rx_irq() {
    if is_rpi3() {
        crate::arch::bcm2836_irq::reenable_miniuart_irq();
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
        // In interrupt context the UART RX handler calls back into the console
        // to echo input.  If the TX FIFO is full we must not spin waiting for
        // the host to drain it, because that would block the interrupt handler
        // and lose incoming bytes.  In process context local IRQs are enabled,
        // so blocking until space is available remains safe.
        if !crate::arch::irq::is_local_enabled() {
            if (miniuart_read_stat() & MINIUART_STAT_TX_SPACE) != 0 {
                miniuart_write(data);
            }
            return;
        }
        while (miniuart_read_stat() & MINIUART_STAT_TX_SPACE) == 0 {}
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
        while (unsafe {
            core::ptr::read_volatile((MINIUART_BASE_PA + MINIUART_STAT_OFFSET) as *const u32)
        } & MINIUART_STAT_TX_SPACE)
            == 0
        {}
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
