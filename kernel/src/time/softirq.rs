// SPDX-License-Identifier: MPL-2.0

use alloc::{boxed::Box, vec, vec::Vec};

use aster_softirq::{softirq_id::TIMER_SOFTIRQ_ID, SoftIrqLine};
use ostd::{sync::RcuOption, timer};

#[expect(clippy::type_complexity)]
static TIMER_SOFTIRQ_CALLBACKS: RcuOption<Box<Vec<fn()>>> = RcuOption::new_none();

pub(super) fn init() {
    ostd::arch::boot::pl011_puts_safe(b"[softirq] init start\n");
    SoftIrqLine::get(TIMER_SOFTIRQ_ID).enable(timer_softirq_handler);

    // Disabled for RPi3 timer-callback debugging.
    // timer::register_callback_on_cpu(|| {
    //     SoftIrqLine::get(TIMER_SOFTIRQ_ID).raise();
    // });
    ostd::arch::boot::pl011_puts_safe(b"[softirq] init done\n");
}

/// Registers a function that will be executed during timer softirq.
pub(super) fn register_callback(func: fn()) {
    ostd::arch::boot::pl011_puts_safe(b"[regcb] enter\n");
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        ostd::arch::boot::pl011_puts_safe(b"[regcb] loop top\n");
        let callbacks = TIMER_SOFTIRQ_CALLBACKS.read();
        ostd::arch::boot::pl011_puts_safe(b"[regcb] read done\n");
        match callbacks.get() {
            // Initialized, copy the vector, push the function and update.
            Some(callbacks_vec) => {
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] some branch\n");
                }
                let mut callbacks_cloned = callbacks_vec.clone();
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] cloned\n");
                }
                callbacks_cloned.push(func);
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] some before cx\n");
                }
                if callbacks.compare_exchange(Some(callbacks_cloned)).is_ok() {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] some cx ok\n");
                    break;
                }
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] some cx failed\n");
                }
            }
            // Uninitialized, initialize it.
            None => {
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] none branch\n");
                }
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] none before cx\n");
                }
                if callbacks
                    .compare_exchange(Some(Box::new(vec![func])))
                    .is_ok()
                {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] none cx ok\n");
                    break;
                }
                if attempt < 5 {
                    ostd::arch::boot::pl011_puts_safe(b"[regcb] none cx failed\n");
                }
            }
        }
        // Contention on initialization or pushing, retry.
        core::hint::spin_loop();
    }
    ostd::arch::boot::pl011_puts_safe(b"[regcb] leave\n");
}

fn timer_softirq_handler() {
    let callbacks = TIMER_SOFTIRQ_CALLBACKS.read();
    if let Some(callbacks) = callbacks.get() {
        for callback in callbacks.iter() {
            (callback)();
        }
    }
}
