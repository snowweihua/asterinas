// SPDX-License-Identifier: MPL-2.0

use alloc::{boxed::Box, string::ToString, sync::Arc};

use aster_framebuffer::{CONSOLE_NAME, FRAMEBUFFER_CONSOLE};
use log::info;
use ostd::arch::serial;
use ostd::arch::trap::TrapFrame;
use ostd::irq::IrqLine;
use ostd::mm::{Infallible, VmReader};
use spin::Once;

const PL011_UART_IRQ: u8 = 33;

static UART_CALLBACK: Once<Box<dyn Fn(VmReader<Infallible>) + Send + Sync>> = Once::new();
static UART_IRQ: Once<IrqLine> = Once::new();

pub fn init() {
    for (name, _) in aster_input::all_devices() {
        info!("Found Input device, name:{}", name);
    }

    if let Some(console) = FRAMEBUFFER_CONSOLE.get() {
        aster_console::register_device(CONSOLE_NAME.to_string(), console.clone());
    }

    #[cfg(target_arch = "aarch64")]
    {
        aster_console::register_device("pl011-uart".to_string(), Arc::new(Pl011Console));
        init_uart_irq();
    }
}

#[cfg(target_arch = "aarch64")]
fn init_uart_irq() {
    UART_IRQ.call_once(|| {
        let mut uart_irq = IrqLine::alloc_specific(PL011_UART_IRQ).unwrap();
        uart_irq.on_active(uart_irq_handler);
        serial::init_rx_irq();
        uart_irq
    });
}

#[cfg(target_arch = "aarch64")]
fn uart_irq_handler(_trapframe: &TrapFrame) {
    while serial::has_data() {
        let byte = serial::receive();
        if let Some(callback) = UART_CALLBACK.get() {
            let ch = [byte];
            let reader = VmReader::<Infallible>::from(ch.as_slice());
            callback(reader);
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[derive(Debug)]
struct Pl011Console;

#[cfg(target_arch = "aarch64")]
impl aster_console::AnyConsoleDevice for Pl011Console {
    fn send(&self, buf: &[u8]) {
        for &byte in buf {
            serial::send(byte);
        }
    }

    fn register_callback(&self, callback: &'static aster_console::ConsoleCallback) {
        UART_CALLBACK.call_once(|| Box::new(callback));
    }
}
