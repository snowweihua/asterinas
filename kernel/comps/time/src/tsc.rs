// SPDX-License-Identifier: MPL-2.0

//! This module provide a instance of `ClockSource` based on TSC.
//!
//! Use `init` to initialize this module.
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

use ostd::{
    arch::{read_tsc, tsc_freq},
    timer::{self, TIMER_FREQ},
};
use ostd::sync::Once;

use crate::{
    clocksource::{ClockSource, Instant},
    START_TIME, VDSO_DATA_HIGH_RES_UPDATE_FN,
};

/// A instance of TSC clocksource.
pub static CLOCK: Once<Arc<ClockSource>> = Once::new();

const MAX_DELAY_SECS: u64 = 100;

/// Init tsc clocksource module.
pub(super) fn init() {
    ostd::arch::boot::pl011_puts_safe(b"[tsc] init start\n");
    init_clock();
    ostd::arch::boot::pl011_puts_safe(b"[tsc] clock inited\n");
    calibrate();
    ostd::arch::boot::pl011_puts_safe(b"[tsc] calibrated\n");
    init_timer();
    ostd::arch::boot::pl011_puts_safe(b"[tsc] timer registered\n");
}

fn init_clock() {
    CLOCK.call_once(|| {
        let freq = tsc_freq();
        ostd::arch::boot::pl011_puts_safe(b"[tsc] freq computed\n");
        Arc::new(ClockSource::new(
            freq,
            MAX_DELAY_SECS,
            Arc::new(read_tsc),
        ))
    });
}

/// Calibrate the TSC and system time based on the RTC time.
fn calibrate() {
    ostd::arch::boot::pl011_puts_safe(b"[tsc] calibrate start\n");
    let clock = CLOCK.get().unwrap();
    let cycles = clock.read_cycles();
    clock.calibrate(cycles);
    ostd::arch::boot::pl011_puts_safe(b"[tsc] read start time\n");
    let start = crate::read();
    ostd::arch::boot::pl011_puts_safe(b"[tsc] start time read\n");
    START_TIME.call_once(|| start);
}

/// Read an `Instant` of tsc clocksource.
pub(super) fn read_instant() -> Instant {
    let clock = CLOCK.get().unwrap();
    clock.read_instant()
}

fn update_clocksource() {
    let clock = CLOCK.get().unwrap();
    clock.update();

    // Update vdso data.
    if let Some(update_fn) = VDSO_DATA_HIGH_RES_UPDATE_FN.get() {
        let (last_instant, last_cycles) = clock.last_record();
        update_fn(last_instant, last_cycles);
    }
}

static TSC_UPDATE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn init_timer() {
    // The `max_delay_secs` should be set as `clock.max_delay_secs() >> 1` or something much smaller than `max_delay_secs`.
    // This is because the initialization of this timer occurs during system startup,
    // and the system will also undergo other initialization processes, during which time interrupts are disabled.
    // This results in the actual trigger time of the timer being delayed by about 5 seconds compared to the set time.
    // If without KVM, the delayed time will be larger.
    // TODO: This is a temporary solution, and should be modified in the future.
    let max_delay_secs = CLOCK.get().unwrap().max_delay_secs() >> 1;
    let delay_counts = TIMER_FREQ * max_delay_secs;

    let update = move || {
        let counter = TSC_UPDATE_COUNTER.fetch_add(1, Ordering::Relaxed);

        if counter % delay_counts == 0 {
            update_clocksource();
        }
    };

    // TODO: re-organize the code structure and use the `Timer` to achieve the updating.
    timer::register_callback_on_cpu(update);
}
