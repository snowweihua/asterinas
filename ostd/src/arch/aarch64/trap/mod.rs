// SPDX-License-Identifier: MPL-2.0

//! Handles trap.

#[expect(clippy::module_inception)]
mod trap;

use spin::Once;
pub(super) use trap::RawUserContext;
pub use trap::TrapFrame;

use super::cpu::context::{CpuException, CpuExceptionInfo};
use crate::{cpu::PrivilegeLevel, irq::call_irq_callback_functions};

/// Initializes interrupt handling on AArch64.
pub(crate) unsafe fn init() {
    unsafe {
        self::trap::init();
    }
}

/// Handle synchronous exceptions from current EL (kernel exceptions).
#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(f: &mut TrapFrame) {
    // Kernel-mode exceptions (e.g. data abort) - for now panic.
    // TODO: handle kernel-mode page faults.
    crate::early_println!(
        "[sec] sync_exception_current: ESR={:#x} ELR={:#x} SPSR={:#x} LR={:#x}",
        f.esr_el1,
        f.elr_el1,
        f.spsr_el1,
        f.lr
    );
    panic!(
        "Kernel synchronous exception! ESR={:#x} ELR={:#x}",
        f.esr_el1, f.elr_el1
    );
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
