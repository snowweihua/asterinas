// SPDX-License-Identifier: MPL-2.0

//! AArch64 console I/O via mini-UART on Raspberry Pi 3B/3B+ (BCM2837).
//!
//! The mini-UART is an auxiliary peripheral at physical address 0x3F215040.
//! Unlike the PL011, it requires explicit GPIO pin configuration and baud
//! rate setup before use.
//!
//! All MMIO accesses use kernel linear map VAs (PA + 0xffff_8000_0000_0000).
//!
//! References:
//! - BCM2835 ARM Peripherals manual, section 2.2 (Mini UART)
//! - BCM2837 is similar to BCM2835 for the auxiliary/mini-UART block

/// Mini-UART base VA in the kernel linear map.
const MINIUART_BASE_VA: usize = 0xffff_8000_3F215000;

/// Auxiliary enable register (offset 0x04 from AUX base, which equals mini-UART base).
const AUX_ENABLES: usize = MINIUART_BASE_VA + 0x04;

/// Mini-UART I/O Data register (offset 0x40).
const AUX_MU_IO_REG: usize = 0x40;

/// Mini-UART Modem Control register (offset 0x50).
const AUX_MU_MCR_REG: usize = 0x50;

/// Mini-UART Line Control register (offset 0x4C).
const AUX_MU_LCR_REG: usize = 0x4C;

/// Mini-UART Interrupt Enable register (offset 0x44).
const AUX_MU_IER_REG: usize = 0x44;

/// Mini-UART Interrupt Identify register (offset 0x48).
const AUX_MU_IIR_REG: usize = 0x48;

/// Mini-UART Baud Rate register (offset 0x68).
const AUX_MU_BAUD_REG: usize = 0x68;

/// Mini-UART Control register (offset 0x60).
const AUX_MU_CNTL_REG: usize = 0x60;

/// Mini-UART Line Status register (offset 0x54).
const AUX_MU_LSR_REG: usize = 0x54;

/// LSR register bits.
const LSR_TX_IDLE: u32 = 1 << 5;

/// GPIO base VA in the kernel linear map (PA 0x3F200000).
const GPIO_BASE_VA: usize = 0xffff_8000_3F200000;

/// GPIO function select register 1 (offset 0x04 within GPIO block).
const GPFSEL1: usize = GPIO_BASE_VA + 0x04;

/// GPIO pull-up/down register (offset 0x94 within GPIO block).
const GPPUD: usize = GPIO_BASE_VA + 0x94;

/// GPIO pull-up/down clock register (offset 0x98 within GPIO block).
const GPPUDCLK0: usize = GPIO_BASE_VA + 0x98;

/// Mini-UART BAUD register value for 115200 baud.
/// Formula: BAUD = (250MHz / (8 * 115200)) - 1 = 270.255... -> 270
const BAUD_115200: u32 = 270;

#[inline(always)]
fn reg_read(addr: usize) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

#[inline(always)]
fn reg_write(addr: usize, val: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, val) }
}

/// Initializes the mini-UART serial port for Raspberry Pi 3B/3B+.
/// This must be called before using mini_uart_send().
pub(crate) fn init() {
    let base = MINIUART_BASE_VA;

    reg_write(AUX_ENABLES, 1);
    reg_read(AUX_ENABLES);

    reg_write(base + AUX_MU_CNTL_REG, 0);

    reg_write(base + AUX_MU_LCR_REG, 3);

    reg_write(base + AUX_MU_MCR_REG, 0);

    reg_write(base + AUX_MU_IER_REG, 0);

    reg_write(base + AUX_MU_IIR_REG, 0xc6);

    reg_write(base + AUX_MU_BAUD_REG, BAUD_115200);

    let r = reg_read(GPFSEL1);
    let r = r & !((7 << 12) | (7 << 15));
    let r = r | (2 << 12) | (2 << 15);
    reg_write(GPFSEL1, r);

    reg_write(GPPUD, 0);
    for _ in 0..150 {
        core::hint::spin_loop();
    }
    reg_write(GPPUDCLK0, (1 << 14) | (1 << 15));
    for _ in 0..150 {
        core::hint::spin_loop();
    }
    reg_write(GPPUD, 0);
    reg_write(GPPUDCLK0, 0);

    reg_read(GPPUD);
    reg_read(GPFSEL1);

    reg_write(base + AUX_MU_CNTL_REG, 3);

    // Flush D-cache lines covering all UART and GPIO registers we wrote above.
    //
    // serial_mini::init() runs while the boot linear-map is active (Normal
    // Write-Back mapping).  All writes above went to the D-cache.  After
    // activate_kernel_page_table() the same PAs are mapped as Normal
    // Non-Cacheable.  Accessing the same PAs with two different cacheability
    // attributes without a cache-maintenance operation is architecturally
    // UNPREDICTABLE (ARM DDI0487 B2.7.2).  On Cortex-A53 this manifests as
    // the D-cache serving stale LSR values (TX_IDLE=0) for Normal NC reads,
    // causing mini_uart_send() to spin forever.
    //
    // DC CIVAC: Clean and Invalidate by Virtual Address to Point of
    // Coherency.  Flushes the line to hardware and removes it from cache so
    // subsequent NC reads go directly to hardware.
    unsafe {
        // AUX base cache line (contains AUX_ENABLES at offset 0x04)
        flush_cache_line(AUX_ENABLES & !63);
        // UART register cache line (offsets 0x40–0x7F: IO, IER, IIR, LCR, MCR, LSR, CNTL, BAUD)
        flush_cache_line((base + AUX_MU_IO_REG) & !63);
        // GPIO GPFSEL1 cache line (offset 0x00 of GPIO block)
        flush_cache_line(GPFSEL1 & !63);
        // GPIO GPPUD / GPPUDCLK0 cache line (offsets 0x94/0x98, 64-byte aligned = 0x80)
        flush_cache_line(GPPUD & !63);
        core::arch::asm!("dsb ish", options(nostack, nomem, preserves_flags));
    }
}

/// Clean and invalidate a single D-cache line by virtual address.
///
/// # Safety
/// The caller must issue a `dsb ish` after all `flush_cache_line` calls.
#[inline(always)]
unsafe fn flush_cache_line(va: usize) {
    unsafe {
        core::arch::asm!(
            "dc civac, {0}",
            in(reg) va as *const u8,
            options(nostack, preserves_flags)
        );
    }
}

/// Sends a byte on the mini-UART serial port.
pub fn mini_uart_send(data: u8) {
    let base = MINIUART_BASE_VA;
    while (reg_read(base + AUX_MU_LSR_REG) & LSR_TX_IDLE) == 0 {}
    reg_write(base + AUX_MU_IO_REG, data as u32);
}
