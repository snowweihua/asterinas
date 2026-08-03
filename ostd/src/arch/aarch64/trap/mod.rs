// SPDX-License-Identifier: MPL-2.0

//! Handles trap.

#[expect(clippy::module_inception)]
mod trap;

use aarch64_cpu::registers::{Readable, ESR_EL1, FAR_EL1};
pub(super) use trap::RawUserContext;
pub use trap::TrapFrame;

use super::cpu::context::{CpuException, CpuExceptionInfo};
use crate::{
    cpu::PrivilegeLevel, irq::call_irq_callback_functions, mm::MAX_USERSPACE_VADDR, sync::Once,
};

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

#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(f: &mut TrapFrame) {
    macro_rules! put_char {
        ($c:expr) => {
            unsafe {
                core::arch::asm!(
                    "movz x4, #0x3F21, lsl #16",
                    "movk x4, #0x5054",
                    "1: ldrb w5, [x4]",
                    "tst w5, #0x20",
                    "beq 1b",
                    "movz x4, #0x3F21, lsl #16",
                    "movk x4, #0x5040",
                    "strb w3, [x4]",
                    in("w3") $c as u32,
                    out("x4") _,
                    out("w5") _,
                    options(nostack),
                );
            }
        };
    }
    macro_rules! put_crlf {
        () => {
            put_char!(b'\r');
            put_char!(b'\n');
        };
    }
    macro_rules! put_hex_nibble {
        ($v:expr) => {{
            let nibble = (($v as usize) & 0xf) as u8;
            let c = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
            put_char!(c);
        }};
    }
    macro_rules! put_hex {
        ($v:expr) => {{
            let mut val = $v as usize;
            for i in (0..16).rev() {
                put_hex_nibble!((val >> (i * 4)) & 0xf);
            }
        }};
    }
    macro_rules! put_str {
        ($s:expr) => {{
            for &ch in $s {
                put_char!(ch);
            }
        }};
    }

    put_crlf!();
    put_str!(b"[EL1-SYNC]");
    put_crlf!();
    put_str!(b" ESR=");
    put_hex!(f.esr_el1);
    put_str!(b" ELR=");
    put_hex!(f.elr_el1);
    put_str!(b" FAR=");
    put_hex!(FAR_EL1.get() as usize);
    put_crlf!();
    put_str!(b" x0=");
    put_hex!(f.general.x0);
    put_str!(b" x1=");
    put_hex!(f.general.x1);
    put_str!(b" x2=");
    put_hex!(f.general.x2);
    put_str!(b" x3=");
    put_hex!(f.general.x3);
    put_crlf!();
    put_str!(b" x4=");
    put_hex!(f.general.x4);
    put_str!(b" x5=");
    put_hex!(f.general.x5);
    put_str!(b" x6=");
    put_hex!(f.general.x6);
    put_str!(b" x7=");
    put_hex!(f.general.x7);
    put_crlf!();
    put_str!(b" x8=");
    put_hex!(f.general.x8);
    put_str!(b" x9=");
    put_hex!(f.general.x9);
    put_str!(b" x10=");
    put_hex!(f.general.x10);
    put_str!(b" x11=");
    put_hex!(f.general.x11);
    put_crlf!();
    put_str!(b" x12=");
    put_hex!(f.general.x12);
    put_str!(b" x13=");
    put_hex!(f.general.x13);
    put_str!(b" x14=");
    put_hex!(f.general.x14);
    put_str!(b" x15=");
    put_hex!(f.general.x15);
    put_crlf!();
    put_str!(b" x16=");
    put_hex!(f.general.x16);
    put_str!(b" x17=");
    put_hex!(f.general.x17);
    put_str!(b" x18=");
    put_hex!(f.general.x18);
    put_str!(b" x19=");
    put_hex!(f.general.x19);
    put_crlf!();
    put_str!(b" x20=");
    put_hex!(f.general.x20);
    put_str!(b" x21=");
    put_hex!(f.general.x21);
    put_str!(b" x22=");
    put_hex!(f.general.x22);
    put_str!(b" x23=");
    put_hex!(f.general.x23);
    put_crlf!();
    put_str!(b" x24=");
    put_hex!(f.general.x24);
    put_str!(b" x25=");
    put_hex!(f.general.x25);
    put_str!(b" x26=");
    put_hex!(f.general.x26);
    put_str!(b" x27=");
    put_hex!(f.general.x27);
    put_crlf!();
    put_str!(b" x28=");
    put_hex!(f.general.x28);
    put_str!(b" x29=");
    put_hex!(f.general.x29);
    put_str!(b" lr=");
    put_hex!(f.lr);
    put_crlf!();
    put_str!(b" spsr=");
    put_hex!(f.spsr_el1);
    put_crlf!();
    put_str!(b"### EL1 HALT ###");
    put_crlf!();
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
