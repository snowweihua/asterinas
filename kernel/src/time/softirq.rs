// SPDX-License-Identifier: MPL-2.0

use alloc::{boxed::Box, vec, vec::Vec};

use aster_softirq::{softirq_id::TIMER_SOFTIRQ_ID, SoftIrqLine};
use ostd::{sync::RcuOption, timer};

#[expect(clippy::type_complexity)]
static TIMER_SOFTIRQ_CALLBACKS: RcuOption<Box<Vec<fn()>>> = RcuOption::new_none();

pub(super) fn init() {
    SoftIrqLine::get(TIMER_SOFTIRQ_ID).enable(timer_softirq_handler);

    timer::register_callback_on_cpu(|| {
        SoftIrqLine::get(TIMER_SOFTIRQ_ID).raise();
    });
}

/// Registers a function that will be executed during timer softirq.
pub(super) fn register_callback(func: fn()) {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let callbacks = TIMER_SOFTIRQ_CALLBACKS.read();
        match callbacks.get() {
            // Initialized, copy the vector, push the function and update.
            Some(callbacks_vec) => {
                if attempt < 5 {
                }
                let mut callbacks_cloned = callbacks_vec.clone();
                if attempt < 5 {
                }
                callbacks_cloned.push(func);
                if attempt < 5 {
                }
                if callbacks.compare_exchange(Some(callbacks_cloned)).is_ok() {
                    break;
                }
                if attempt < 5 {
                }
            }
            // Uninitialized, initialize it.
            None => {
                if attempt < 5 {
                }
                if attempt < 5 {
                }
                if callbacks
                    .compare_exchange(Some(Box::new(vec![func])))
                    .is_ok()
                {
                    break;
                }
                if attempt < 5 {
                }
            }
        }
        // Contention on initialization or pushing, retry.
        core::hint::spin_loop();
    }
}

fn timer_softirq_handler() {
    let callbacks = TIMER_SOFTIRQ_CALLBACKS.read();
    if let Some(callbacks) = callbacks.get() {
        for callback in callbacks.iter() {
            (callback)();
        }
    }
}
