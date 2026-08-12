// SPDX-License-Identifier: MPL-2.0

use core::time::Duration;

use log::{Metadata, Record};
use ostd::timer::Jiffies;

/// The logger used for Asterinas.
struct AsterLogger;

static LOGGER: AsterLogger = AsterLogger;

impl log::Log for AsterLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Suppress noisy Info/Debug/Trace logs while bringing up RPi3.
        // Use println!/print! for deliberate debug markers instead.
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &Record) {
        let timestamp = Jiffies::elapsed().as_duration();
        print_logs(record, &timestamp);
    }

    fn flush(&self) {}
}

#[cfg(feature = "log_color")]
fn print_logs(record: &Record, timestamp: &Duration) {
    use owo_colors::Style;

    let secs = timestamp.as_secs();
    let millis = timestamp.subsec_millis();

    let timestamp_style = Style::new().green();
    let record_style = Style::new().default_color();
    let level_style = match record.level() {
        log::Level::Error => Style::new().red(),
        log::Level::Warn => Style::new().bright_yellow(),
        log::Level::Info => Style::new().blue(),
        log::Level::Debug => Style::new().bright_green(),
        log::Level::Trace => Style::new().bright_black(),
    };

    super::_print(format_args!(
        "{} {:<5}: {}\n",
        timestamp_style.style(format_args!("[{:>6}.{:03}]", secs, millis)),
        level_style.style(record.level()),
        record_style.style(record.args())
    ));
}

#[cfg(not(feature = "log_color"))]
fn print_logs(record: &Record, _timestamp: &Duration) {
    // During RPi3 bring-up we only keep Warn/Error enabled and use them as
    // short serial markers. Emit just the message so it survives the
    // mini-UART TX path without truncation.
    super::_print(format_args!("{}\n", record.args()));
}

pub(super) fn init() {
    ostd::logger::inject_logger(&LOGGER);
}
