// SPDX-License-Identifier: MPL-2.0

//! Handles trap.

#[expect(clippy::module_inception)]
mod trap;

use core::sync::atomic::Ordering;

use arm_gic::gicv3::{GicCpuInterface, GicV3, registers::*};

use spin::Once;
pub(super) use trap::RawUserContext;
pub use trap::TrapFrame;

use super::{cpu::context::CpuExceptionInfo, timer::TIMER_IRQ_NUM};
use crate::{cpu::PrivilegeLevel, irq::call_irq_callback_functions};

/// Initializes interrupt handling on RISC-V.
pub(crate) unsafe fn init() {
    unsafe {
        self::trap::init();
    }
}

/// Handle traps (only from kernel).

#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(f: &mut TrapFrame) {
    // handle_svc()  --> syscall
}

// An instance of GicV3 created during system setup.
static mut GIC: Option<GicV3> = None;
extern "C" fn irq_current(f: &mut TrapFrame) {
    let gic_cpu_iface = unsafe {
        // Assume GIC instance is initialized and accessible
        GIC.as_mut().unwrap().cpu_interface()
    };

    // Read the IAR to get the IRQ ID
    let irq = gic_cpu_iface.read_iar1();
    call_irq_callback_functions(f, irq as _, PrivilegeLevel::Kernel);

    // Write to the EOI register
    gic_cpu_iface.write_eoir1(irq);
}
extern "C" fn fiq_current(f: &mut TrapFrame) {
    // todo 
}
extern "C" fn serr_current(f: &mut TrapFrame) {

}
extern "C" fn sync_lower(f: &mut TrapFrame) {
    panic!("Unexpected sync_lower");
}
extern "C" fn irq_lower(f: &mut TrapFrame) {
    panic!("Unexpected irq_lower");
}
extern "C" fn fiq_lower(f: &mut TrapFrame) {
    panic!("Unexpected fiq_lower");
}
extern "C" fn serr_lower(f: &mut TrapFrame) {
    panic!("Unexpected serr_lower");
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
