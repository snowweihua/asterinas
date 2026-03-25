// SPDX-License-Identifier: MPL-2.0

use alloc::{string::ToString, sync::Arc};

use aster_framebuffer::{CONSOLE_NAME, FRAMEBUFFER_CONSOLE};
use log::info;

pub fn init() {
    // print all the input device to make sure input crate will compile
    for (name, _) in aster_input::all_devices() {
        info!("Found Input device, name:{}", name);
    }

    if let Some(console) = FRAMEBUFFER_CONSOLE.get() {
        aster_console::register_device(CONSOLE_NAME.to_string(), console.clone());
    }

    #[cfg(target_arch = "aarch64")]
    {
        // On AArch64 there is no framebuffer or virtio-console; register a minimal
        // PL011 UART console so that the TTY subsystem has at least one device.
        aster_console::register_device(
            "pl011-uart".to_string(),
            Arc::new(Pl011Console),
        );
    }
}

/// A minimal `AnyConsoleDevice` backed by the AArch64 PL011 UART.
///
/// Only transmit is supported; `register_callback` is a no-op because the
/// early-boot PL011 is not wired up to an interrupt handler yet.
#[cfg(target_arch = "aarch64")]
#[derive(Debug)]
struct Pl011Console;

#[cfg(target_arch = "aarch64")]
impl aster_console::AnyConsoleDevice for Pl011Console {
    fn send(&self, buf: &[u8]) {
        for &byte in buf {
            ostd::arch::serial::send(byte);
        }
    }

    fn register_callback(&self, _callback: &'static aster_console::ConsoleCallback) {
        // No receive interrupt support yet on AArch64.
    }
}
