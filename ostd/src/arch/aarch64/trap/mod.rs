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
    raw_puts(b"\r\n[EL1-SYNC]\r\n");

    let esr = ESR_EL1.get() as usize;
    let ec = (esr >> 26) & 0x3f;

    // Always print raw hex to both UARTs — board detection may not have run yet.
    raw_puts(b" ESR=");
    raw_put_hex(esr);
    raw_puts(b"\r\n ELR=");
    raw_put_hex(f.elr_el1);
    raw_puts(b"\r\n LR=");
    raw_put_hex(f.lr);
    raw_puts(b"\r\n SPSR=");
    raw_put_hex(f.spsr_el1);

    raw_puts(b"\r\n=== x0-x30 ===\r\n");
    raw_puts(b" x0="); raw_put_hex(f.general.x0);
    raw_puts(b" x1="); raw_put_hex(f.general.x1);
    raw_puts(b" x2="); raw_put_hex(f.general.x2);
    raw_puts(b" x3="); raw_put_hex(f.general.x3);
    raw_puts(b" x4="); raw_put_hex(f.general.x4);
    raw_puts(b" x5="); raw_put_hex(f.general.x5);
    raw_puts(b" x6="); raw_put_hex(f.general.x6);
    raw_puts(b" x7="); raw_put_hex(f.general.x7);
    raw_puts(b" x8="); raw_put_hex(f.general.x8);
    raw_puts(b" x9="); raw_put_hex(f.general.x9);
    raw_puts(b" x10="); raw_put_hex(f.general.x10);
    raw_puts(b" x11="); raw_put_hex(f.general.x11);
    raw_puts(b" x12="); raw_put_hex(f.general.x12);
    raw_puts(b" x13="); raw_put_hex(f.general.x13);
    raw_puts(b" x14="); raw_put_hex(f.general.x14);
    raw_puts(b" x15="); raw_put_hex(f.general.x15);
    raw_puts(b" x16="); raw_put_hex(f.general.x16);
    raw_puts(b" x17="); raw_put_hex(f.general.x17);
    raw_puts(b" x18="); raw_put_hex(f.general.x18);
    raw_puts(b" x19="); raw_put_hex(f.general.x19);
    raw_puts(b" x20="); raw_put_hex(f.general.x20);
    raw_puts(b" x21="); raw_put_hex(f.general.x21);
    raw_puts(b" x22="); raw_put_hex(f.general.x22);
    raw_puts(b" x23="); raw_put_hex(f.general.x23);
    raw_puts(b" x24="); raw_put_hex(f.general.x24);
    raw_puts(b" x25="); raw_put_hex(f.general.x25);
    raw_puts(b" x26="); raw_put_hex(f.general.x26);
    raw_puts(b" x27="); raw_put_hex(f.general.x27);
    raw_puts(b" x28="); raw_put_hex(f.general.x28);
    raw_puts(b" x29="); raw_put_hex(f.general.x29);
    raw_puts(b" x30="); raw_put_hex(f.lr);
    raw_puts(b"\r\n");

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
        }
        _ => {
            raw_puts(b"\r\n");
        }
    }

    raw_puts(b"\r\n### EL1 HALT ###\r\n");
    loop {
        core::hint::spin_loop();
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
    raw_puts(b"\r\n[EL1-SERR]\r\n");
    raw_puts(b" ESR=");
    raw_put_hex(f.esr_el1);
    raw_puts(b" ELR=");
    raw_put_hex(f.elr_el1);
    raw_puts(b" SPSR=");
    raw_put_hex(f.spsr_el1);
    raw_puts(b" x30=");
    raw_put_hex(f.lr);
    raw_puts(b"\r\n### EL1 SERR HALT ###\r\n");
    loop {
        core::hint::spin_loop();
    }
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
