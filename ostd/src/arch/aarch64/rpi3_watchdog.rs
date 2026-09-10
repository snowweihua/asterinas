// SPDX-License-Identifier: MPL-2.0

//! Raspberry Pi 3 hardware watchdog.
//!
//! The BCM2837 PM block contains a watchdog that the GPU firmware may leave
//! running. If the ARM core stops kicking it, the board resets. This module
//! provides a simple kick function intended to be called from the periodic
//! timer interrupt.

use core::sync::atomic::Ordering;

use crate::{arch::board::IS_HARDWARE, mm::paddr_to_vaddr};

/// BCM2837 Power Management block base address.
const PM_BASE_PA: usize = 0x3F10_0000;

/// Watchdog timer register offset.
const PM_WDOG_OFFSET: usize = 0x24;

/// Watchdog reset control register offset.
const PM_RSTC_OFFSET: usize = 0x1c;

/// PM register write password (top 8 bits).
const PM_PASSWORD: u32 = 0x5a00_0000;

/// Watchdog configured for full reset.
const PM_RSTC_WRCFG_FULL_RESET: u32 = 0x20;
/// Mask to clear the WRCFG field while preserving all other PM_RSTC bits.
const PM_RSTC_WRCFG_CLR: u32 = 0xffffffcf;

/// Maximum watchdog timeout value (20-bit PM_WDOG register).
///
/// The BCM2837 watchdog ticks at 65536 Hz, so:
///   timeout_seconds = ticks / 65536
/// With 0xFFFFF ticks: 1048575 / 65536 ≈ 16 seconds per kick.
/// The Linux kernel bcm2835_wdt driver uses `timeout_seconds * 0x10000` ticks.
const PM_WDOG_MAX_TICKS: u32 = 0x000F_FFFF;

/// Re-arm / kick the Raspberry Pi 3 hardware watchdog.
///
/// Writes both RSTC (WRCFG=full_reset) and WDOG (maximum ~16s timeout),
/// matching the Linux bcm2835_wdt keepalive sequence. Safe to call any time.
/// No-op on QEMU/non-hardware.
pub fn kick() {
    if !IS_HARDWARE.load(Ordering::Relaxed) {
        return;
    }
    let base_va = paddr_to_vaddr(PM_BASE_PA);
    unsafe {
        let cur = core::ptr::read_volatile((base_va + PM_RSTC_OFFSET) as *const u32);
        core::ptr::write_volatile(
            (base_va + PM_RSTC_OFFSET) as *mut u32,
            PM_PASSWORD | (cur & PM_RSTC_WRCFG_CLR) | PM_RSTC_WRCFG_FULL_RESET,
        );
        core::ptr::write_volatile(
            (base_va + PM_WDOG_OFFSET) as *mut u32,
            PM_PASSWORD | PM_WDOG_MAX_TICKS,
        );
    }
}

/// Set watchdog to minimum timeout (~15us at 1MHz clock).
/// Used for testing if the watchdog register writes are working.
/// No-op on QEMU/non-hardware.
pub fn test_min_timeout() {
    if !IS_HARDWARE.load(Ordering::Relaxed) {
        return;
    }
    let base_va = paddr_to_vaddr(PM_BASE_PA);
    unsafe {
        core::ptr::write_volatile((base_va + PM_WDOG_OFFSET) as *mut u32, PM_PASSWORD | 0x0001);
        core::ptr::write_volatile(
            (base_va + PM_RSTC_OFFSET) as *mut u32,
            PM_PASSWORD | PM_RSTC_WRCFG_FULL_RESET | 0x01,
        );
    }
}

/// Disable the watchdog (set WRCFG=0, stopping reset generation).
/// Used during initramfs extraction where we kick the watchdog manually
/// but don't want the HW timer to fire.
/// No-op on QEMU/non-hardware.
pub fn disable() {
    if !IS_HARDWARE.load(Ordering::Relaxed) {
        return;
    }
    let base_va = paddr_to_vaddr(PM_BASE_PA);
    unsafe {
        core::ptr::write_volatile((base_va + PM_RSTC_OFFSET) as *mut u32, PM_PASSWORD | 0x0);
    }
}

/// Re-enable the watchdog with the maximum timeout and full reset.
/// No-op on QEMU/non-hardware.
pub fn enable() {
    if !IS_HARDWARE.load(Ordering::Relaxed) {
        return;
    }
    let base_va = paddr_to_vaddr(PM_BASE_PA);
    unsafe {
        core::ptr::write_volatile(
            (base_va + PM_RSTC_OFFSET) as *mut u32,
            PM_PASSWORD | PM_RSTC_WRCFG_FULL_RESET,
        );
        core::ptr::write_volatile(
            (base_va + PM_WDOG_OFFSET) as *mut u32,
            PM_PASSWORD | PM_WDOG_MAX_TICKS,
        );
    }
}

static PREV_TIMEOUT: spin::Once<u32> = spin::Once::new();

/// Configure and arm the watchdog with the maximum timeout (~16 seconds).
///
/// Sets the reset configuration (WRCFG=full_reset) and reloads the counter
/// to the maximum 20-bit value. Call once before long-running operations such
/// as initramfs extraction; thereafter use `kick()` to keep the dog fed.
pub fn set_timeout_long() {
    kick();
}

/// Restore the watchdog timeout to its previous value.
/// No-op on QEMU/non-hardware.
pub fn restore_timeout() {
    if !IS_HARDWARE.load(Ordering::Relaxed) {
        return;
    }
    let base_va = paddr_to_vaddr(PM_BASE_PA);
    if let Some(prev) = PREV_TIMEOUT.get() {
        unsafe {
            core::ptr::write_volatile((base_va + PM_WDOG_OFFSET) as *mut u32, PM_PASSWORD | prev);
        }
    }
}
