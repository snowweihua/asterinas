// SPDX-License-Identifier: MPL-2.0

//! Handles trap.

#[expect(clippy::module_inception)]
mod trap;

use aarch64_cpu::registers::{Readable, ESR_EL1, FAR_EL1};
use spin::Once;
pub(super) use trap::RawUserContext;
pub use trap::TrapFrame;

use super::cpu::context::{CpuException, CpuExceptionInfo};
use crate::{cpu::PrivilegeLevel, irq::call_irq_callback_functions, mm::MAX_USERSPACE_VADDR};

/// Initializes interrupt handling on AArch64.
pub(crate) unsafe fn init() {
    unsafe {
        self::trap::init();
    }
}

/// Write a hex nibble to both RPi3 PL011 and QEMU PL011 directly.
#[inline(always)]
fn raw_put_hex_nibble(v: u8) {
    let ch: u8 = if v < 10 { b'0' + v } else { b'a' + v - 10 };
    unsafe {
        core::arch::asm!(
            "movz x28, #0x3F20, lsl #16",
            "movk x28, #0x1000",
            "strb w27, [x28]",
            "movz x28, #0x0900, lsl #16",
            "strb w27, [x28]",
            in("w27") ch as u32,
            out("x28") _,
            options(nostack),
        );
    }
}

/// Print a usize as 16 hex digits to both UARTs unconditionally.
fn raw_put_hex(v: usize) {
    for shift in (0..64).rev().step_by(4) {
        raw_put_hex_nibble(((v >> shift) & 0xf) as u8);
    }
}

/// Print a static string byte-by-byte to both UARTs unconditionally.
fn raw_puts(s: &[u8]) {
    for &b in s {
        unsafe {
            core::arch::asm!(
                "movz x28, #0x3F20, lsl #16",
                "movk x28, #0x1000",
                "strb w27, [x28]",
                "movz x28, #0x0900, lsl #16",
                "strb w27, [x28]",
                in("w27") b as u32,
                out("x28") _,
                options(nostack),
            );
        }
    }
}

/// Handle synchronous exceptions from current EL (kernel exceptions).
#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(f: &mut TrapFrame) {
    let esr = ESR_EL1.get() as usize;
    let ec = (esr >> 26) & 0x3f;

    // Always print raw hex to both UARTs — board detection may not have run yet.
    raw_puts(b"\r\n[sec] ESR=");
    raw_put_hex(esr);
    raw_puts(b" ELR=");
    raw_put_hex(f.elr_el1);
    raw_puts(b" LR=");
    raw_put_hex(f.lr);

    match ec {
        0x24 | 0x25 => {
            let far = FAR_EL1.get() as usize;
            raw_puts(b" FAR=");
            raw_put_hex(far);
            raw_puts(b"\r\n");
            if far < MAX_USERSPACE_VADDR {
                let cpu_exception = CpuExceptionInfo {
                    code: if ec == 0x24 {
                        CpuException::DataAbortLowerEL
                    } else {
                        CpuException::DataAbortCurrentEL
                    },
                    page_fault_addr: far,
                    error_code: esr,
                };
                if let Some(handler) = USER_PAGE_FAULT_HANDLER.get() {
                    if handler(&cpu_exception).is_ok() {
                        return;
                    }
                }
            }
            crate::early_println!(
                "[sec] sync_exception_current: ESR={:#x} FAR={:#x} ELR={:#x} SPSR={:#x}",
                esr,
                far,
                f.elr_el1,
                f.spsr_el1
            );
            panic!(
                "Kernel synchronous exception! ESR={:#x} FAR={:#x} ELR={:#x}",
                esr, far, f.elr_el1
            );
        }
        _ => {
            raw_puts(b"\r\n");
            crate::early_println!(
                "[sec] sync_exception_current: ESR={:#x} ELR={:#x} SPSR={:#x} LR={:#x}",
                esr,
                f.elr_el1,
                f.spsr_el1,
                f.lr
            );
            panic!(
                "Kernel synchronous exception! ESR={:#x} ELR={:#x}",
                esr, f.elr_el1
            );
        }
    }
}

/// Handle IRQ from current EL.
#[unsafe(no_mangle)]
extern "C" fn irq_current(f: &mut TrapFrame) {
    if let Some(irq_num) = super::gic::acknowledge_interrupt() {
        call_irq_callback_functions(f, irq_num, PrivilegeLevel::Kernel);
    }
}

/// Handle FIQ from current EL.
#[unsafe(no_mangle)]
extern "C" fn fiq_current(_f: &mut TrapFrame) {
    // FIQ not used in Asterinas; ignore.
}

/// Handle SError from current EL.
#[unsafe(no_mangle)]
extern "C" fn serr_current(f: &mut TrapFrame) {
    panic!("SError (system error) at current EL: {:?}", f);
}

/// Handle synchronous exception from lower EL (user mode).
/// This is the user-space exception entry: SVC, data abort, etc.
#[unsafe(no_mangle)]
extern "C" fn sync_lower(f: &mut TrapFrame) {
    // User-mode exceptions are handled by UserContextApiInternal::execute().
    // This handler should never be reached during normal operation because
    // run_user() returns via eret and the kernel re-examines the ESR.
    panic!(
        "Unexpected sync_lower: esr={:#x} elr={:#x}",
        f.esr_el1, f.elr_el1
    );
}

/// Handle IRQ from lower EL.
#[unsafe(no_mangle)]
extern "C" fn irq_lower(f: &mut TrapFrame) {
    if let Some(irq_num) = super::gic::acknowledge_interrupt() {
        call_irq_callback_functions(f, irq_num, PrivilegeLevel::User);
    }
}

/// Handle FIQ from lower EL.
#[unsafe(no_mangle)]
extern "C" fn fiq_lower(_f: &mut TrapFrame) {
    // FIQ not used; ignore.
}

/// Handle SError from lower EL.
#[unsafe(no_mangle)]
extern "C" fn serr_lower(f: &mut TrapFrame) {
    panic!("SError from lower EL: {:?}", f);
}

#[expect(clippy::type_complexity)]
static USER_PAGE_FAULT_HANDLER: Once<fn(&CpuExceptionInfo) -> core::result::Result<(), ()>> =
    Once::new();

/// Injects a custom handler for page faults that occur in the kernel and
/// are caused by user-space address.
pub fn inject_user_page_fault_handler(
    handler: fn(info: &CpuExceptionInfo) -> core::result::Result<(), ()>,
) {
    USER_PAGE_FAULT_HANDLER.call_once(|| handler);
}
