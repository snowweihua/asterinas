// SPDX-License-Identifier: MPL-2.0

//! The timer support.

use core::{
    arch::asm,
    sync::atomic::{AtomicU64, AtomicU8, Ordering},
};

use spin::Once;

use crate::{
    arch::{boot::DEVICE_TREE, trap::TrapFrame},
    irq::IrqLine,
    timer::TIMER_FREQ,
};

static TIMER_IRQ: Once<IrqLine> = Once::new();
pub(super) static TIMER_IRQ_NUM: AtomicU8 = AtomicU8::new(0);

static TIMEBASE_FREQ: AtomicU64 = AtomicU64::new(0);
static TIMER_INTERVAL: AtomicU64 = AtomicU64::new(0);

/// Initializes the timer module.
///
/// # Safety
///
/// This function is safe to call on the following conditions:
/// 1. It is called once and at most once at a proper timing in the boot context.
/// 2. It is called before any other public functions of this module is called.
pub(super) unsafe fn init() {
    #[cfg(target_arch = "aarch64")]
    {
        return;
    }

    TIMEBASE_FREQ.store(
        DEVICE_TREE
            .get()
            .unwrap()
            .cpus()
            .next()
            .unwrap()
            .timebase_frequency() as u64,
        Ordering::Relaxed,
    );
    TIMER_INTERVAL.store(
        TIMEBASE_FREQ.load(Ordering::Relaxed) / TIMER_FREQ,
        Ordering::Relaxed,
    );
    TIMER_IRQ.call_once(|| {
        let mut timer_irq = IrqLine::alloc().unwrap();
        TIMER_IRQ_NUM.store(timer_irq.num(), Ordering::Relaxed);
        timer_irq.on_active(timer_callback);

        timer_irq
    });

    set_next_timer();

}

fn timer_callback(trapframe: &TrapFrame) {
    crate::timer::call_timer_callback_functions(trapframe);

    set_next_timer();
}

fn set_next_timer() {
    // SAFETY: Calling the `SET_NEXT_TIMER_FN` function pointer is safe here
    // because we ensure that it is set to a valid function during the timer
    // initialization, and we never modify it after that.
    unsafe {
        SET_NEXT_TIMER_FN();
    }
}

static mut SET_NEXT_TIMER_FN: fn() = set_next_timer_arch;

fn set_next_timer_arch() {
    let next = get_next_when();
    unsafe {
        asm!("msr cntv_cval_el0, {0}", in(reg) next, options(nostack, nomem, preserves_flags));
        asm!("msr cntv_ctl_el0, {0}", in(reg) 1u64, options(nostack, nomem, preserves_flags));
    }
}

fn get_next_when() -> u64 {
    let current: u64;
    unsafe {
        asm!("mrs {0}, cntvct_el0", out(reg) current, options(nostack, nomem, preserves_flags));
    }
    let interval = TIMER_INTERVAL.load(Ordering::Relaxed);
    current + interval
}

pub(crate) fn get_timebase_freq() -> u64 {
    TIMEBASE_FREQ.load(Ordering::Relaxed)
}
