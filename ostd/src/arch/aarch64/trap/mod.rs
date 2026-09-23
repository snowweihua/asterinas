// SPDX-License-Identifier: MPL-2.0

//! Handles trap.
//!
//! # Safety
//!
//! Trap entry/exit is hand-written assembly that saves and restores the
//! full register state; Rust code here only runs with a valid trap frame.
//! The synchronous-exception dump path is lock- and allocation-free
//! (raw MPIDR read, volatile UART), so dumping state cannot deadlock the
//! faulting CPU.

#[expect(clippy::module_inception)]
mod trap;

use aarch64_cpu::registers::{Readable, ESR_EL1, FAR_EL1};
pub(super) use trap::RawUserContext;
pub use trap::TrapFrame;

use super::cpu::context::{CpuException, CpuExceptionInfo};
use crate::{
    cpu::PrivilegeLevel, irq::call_irq_callback_functions, mm::MAX_USERSPACE_VADDR,
};
use spin::Once;

/// Initializes interrupt handling on AArch64.
pub(crate) unsafe fn init() {
    unsafe {
        self::trap::init();
    }
}

pub(crate) unsafe fn init_on_cpu() {
    unsafe {
        self::trap::init_on_cpu();
    }
}

/// Base addresses (PL011 data register) for lock-free fault dumps.
///
/// Physical addresses stop working once the kernel page table is active on
/// the BSP (it has no low-half mappings), while high-half aliases only work
/// with the MMU on. Select based on SCTLR_EL1.M so dumps work in both phases.
#[inline(always)]
fn raw_uart_bases() -> (usize, usize) {
    const RPI_PA: usize = 0x3F201000;
    const QEMU_PA: usize = 0x09000000;
    let sctlr: usize;
    unsafe {
        core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr, options(nostack, preserves_flags));
    }
    if sctlr & 1 != 0 {
        let linear = crate::mm::kspace::LINEAR_MAPPING_BASE_VADDR;
        (RPI_PA + linear, QEMU_PA + linear)
    } else {
        (RPI_PA, QEMU_PA)
    }
}

/// Upper bound for spinning on the PL011 flag register in fault dumps.
///
/// Mirrors the bounded poll in the regular console path: a stuck status bit
/// must degrade output, never wedge the machine while reporting a fault.
const RAW_FR_POLL_LIMIT: u32 = 10_000_000;
const RAW_FR_TXFF: u32 = 1 << 5;

#[inline(always)]
fn raw_send_byte(base: usize, b: u8) {
    let mut polls = 0;
    loop {
        crate::arch::serial::cache_invalidate_va(base + 0x18);
        let fr = unsafe { core::ptr::read_volatile((base + 0x18) as *const u32) };
        polls += 1;
        if fr & RAW_FR_TXFF == 0 || polls >= RAW_FR_POLL_LIMIT {
            break;
        }
    }
    unsafe {
        core::ptr::write_volatile(base as *mut u32, b as u32);
    }
    crate::arch::serial::cache_clean_va(base);
}

/// Write a hex nibble directly to the fault-dump UART.
#[inline(always)]
fn raw_put_hex_nibble(v: u8) {
    let ch: u8 = if v < 10 { b'0' + v } else { b'a' + v - 10 };
    raw_send_byte(raw_uart_bases().0, ch);
}

/// Print a usize as 16 hex digits to both UARTs unconditionally.
fn raw_put_hex(v: usize) {
    for shift in (0..64).rev().step_by(4) {
        raw_put_hex_nibble(((v >> shift) & 0xf) as u8);
    }
}

/// Print a static string byte-by-byte to the fault-dump UART.
fn raw_puts(s: &[u8]) {
    for &b in s {
        raw_send_byte(raw_uart_bases().0, b);
    }
}

fn sync_exception_dump_once(f: &mut TrapFrame) -> bool {
    macro_rules! put_char {
        ($c:expr) => {
            raw_send_byte(raw_uart_bases().0, $c);
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

    // Attempt to recover from a fallible kernel access to user-space memory
    // before dumping the full EL1 state. If the faulting address is in user
    // space, give the user page-fault handler a chance to map it. If that
    // fails (or is absent), try the exception-table recovery address for the
    // faulting instruction so that functions like __memcpy_fallible can return
    // an EFAULT-style failure instead of hanging the kernel.
    let esr = f.esr_el1;
    let elr = f.elr_el1;
    let far = FAR_EL1.get() as usize;
    if matches!(
        CpuException::from_esr(esr, far),
        CpuException::DataAbortCurrentEL(..)
    ) && far < MAX_USERSPACE_VADDR
    {
        crate::mm::fault::handle_user_page_fault(
            f,
            &CpuException::from_esr(esr, far),
            far,
        );
        return true;
    }

    // Diagnostics are emitted on the MARKER channel (short, spaced, proven to
    // survive the lossy USB serial), BEFORE marker suppression is enabled.
    macro_rules! m {
        ($c:expr) => {
            crate::arch::serial::marker($c)
        };
    }
    macro_rules! mhex {
        ($v:expr) => {
            crate::arch::serial::marker_hex($v as usize)
        };
    }
    m!(b'F');
    mhex!(f.esr_el1);
    mhex!(f.elr_el1);
    mhex!(FAR_EL1.get() as usize);
    let ttbr0: usize;
    let ttbr1: usize;
    unsafe {
        core::arch::asm!("mrs {0}, ttbr0_el1", out(reg) ttbr0, options(nostack, preserves_flags));
        core::arch::asm!("mrs {0}, ttbr1_el1", out(reg) ttbr1, options(nostack, preserves_flags));
    }
    mhex!(ttbr0);
    mhex!(ttbr1);
    // AT-walk the faulting address: F=0 => translation found (stale
    // TLB/walk-cache); F=1 => the table really lacks the entry.
    let par: usize;
    unsafe {
        core::arch::asm!(
            "at s1e1r, {far}",
            "isb",
            "mrs {par}, par_el1",
            far = in(reg) far,
            par = out(reg) par,
            options(nostack, preserves_flags),
        );
    }
    mhex!(par);
    // Top-level slot probes in the active TTBR1 root: linear (256),
    // frame-meta (448), kernel (511). F=0 => present; F=1 level 0 => absent.
    macro_rules! slot {
        ($va:expr) => {{
            let p: usize;
            unsafe {
                core::arch::asm!(
                    "at s1e1r, {va}",
                    "isb",
                    "mrs {p}, par_el1",
                    va = in(reg) $va,
                    p = out(reg) p,
                    options(nostack, preserves_flags),
                );
            }
            mhex!(p);
        }};
    }
    slot!(0xffff_8000_0000_0000usize);
    slot!(0xffff_e000_0000_0000usize);
    slot!(0xffff_ffff_0000_0000usize);
    // The KPT singleton root paddr — does the faulting root match it?
    match crate::mm::kspace::kernel_page_table_root_paddr() {
        Some(pa) => mhex!(pa),
        None => m!(b'x'),
    }
    // CPU + task pointer.
    let mpidr: u64;
    unsafe {
        core::arch::asm!("mrs {0}, mpidr_el1", out(reg) mpidr, options(nostack, nomem, preserves_flags))
    };
    let task_ptr = crate::task::Task::current()
        .map(|t| &*t as *const crate::task::Task as usize)
        .unwrap_or(0);
    mhex!((mpidr & 0x3) as usize);
    mhex!(task_ptr);
    mhex!(f.general.x0);
    mhex!(f.general.x1);

    crate::arch::serial::set_marker_suppressed(true);
    put_crlf!();
    put_str!(b"[EL1-SYNC]");
    put_crlf!();
    put_str!(b"### EL1 HALT ###");
    put_crlf!();
    false
}

#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(f: &mut TrapFrame) {
    // TEMP-HW-DEBUG: EL1 sync-exception census (every 4th) to distinguish a
    // silent refault storm from a halt/spin. Revert before MR-1.
    use core::sync::atomic::{AtomicU64, Ordering};
    static SYNC_COUNT: AtomicU64 = AtomicU64::new(0);
    let n = SYNC_COUNT.fetch_add(1, Ordering::Relaxed);
    if n % 4096 == 0 {
        crate::arch::serial::marker(b'S');
    }
    if !sync_exception_dump_once(&mut *f) {
        loop {
            core::hint::spin_loop();
        }
    }
}

/// Handle IRQ from current EL.
#[unsafe(no_mangle)]
extern "C" fn irq_current(f: &mut TrapFrame) {
    // TEMP-HW-DEBUG: IRQ census (every 65536th) to detect an IRQ storm.
    // Revert before MR-1.
    use core::sync::atomic::{AtomicU64, Ordering};
    static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
    let n = IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
    if n & 0xfffff == 0 {
        crate::arch::serial::marker(b'I');
    }
    if let Some(irq_num) = super::gic::acknowledge_interrupt() {
        let hw_irq_line = crate::arch::irq::HwIrqLine::new(irq_num as u8);
        call_irq_callback_functions(f, &hw_irq_line, PrivilegeLevel::Kernel);
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
        let hw_irq_line = crate::arch::irq::HwIrqLine::new(irq_num as u8);
        call_irq_callback_functions(f, &hw_irq_line, PrivilegeLevel::User);
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
