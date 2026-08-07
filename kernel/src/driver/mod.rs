// SPDX-License-Identifier: MPL-2.0

use alloc::{boxed::Box, string::ToString, sync::Arc};

use aster_framebuffer::{CONSOLE_NAME, FRAMEBUFFER_CONSOLE};
use log::info;
use ostd::arch::serial;
use ostd::arch::trap::TrapFrame;
use ostd::irq::IrqLine;
use ostd::mm::{Infallible, VmReader};
use ostd::timer;
use spin::Once;

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
        aster_console::register_device("serial-uart".to_string(), Arc::new(SerialConsole));
        init_uart_irq();
        // The RPi3 VideoCore firmware can clobber the AUX enable bit in
        // ENABLE_IRQS_1.  Poll the RX FIFO on the 1 ms timer tick so that
        // serial input works even when the AUX IRQ is transiently disabled.
        timer::register_callback_on_cpu(poll_uart_input);
    }
}

#[cfg(target_arch = "aarch64")]
fn init_uart_irq() {
    UART_IRQ.call_once(|| {
        let mut uart_irq = IrqLine::alloc_specific(serial::irq_num()).unwrap();
        uart_irq.on_active(uart_irq_handler);
        serial::init_rx_irq();
        uart_irq
    });
}

#[cfg(target_arch = "aarch64")]
fn uart_irq_handler(_trapframe: &TrapFrame) {
    poll_uart_input();
}

#[cfg(target_arch = "aarch64")]
fn poll_uart_input() {
    // Re-enable the AUX interrupt in case the firmware cleared it.  If the
    // interrupt path is alive the next byte will trigger the handler; if not,
    // the polling below still drains the FIFO.
    serial::reenable_rx_irq();

    static POLL_CNT: core::sync::atomic::AtomicUsize =
        core::sync::atomic::AtomicUsize::new(0);
    let cnt = POLL_CNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if cnt % 1000 == 0 {
        ostd::console::early_print(format_args!("[poll_uart] alive\n"));
    }

    while serial::has_data() {
        let byte = serial::receive();
        ostd::console::early_print(format_args!("[poll_uart] byte={:#x}\n", byte));
        if let Some(callback) = UART_CALLBACK.get() {
            let ch = [byte];
            let reader = VmReader::<Infallible>::from(ch.as_slice());
            callback(reader);
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[derive(Debug)]
struct SerialConsole;

#[cfg(target_arch = "aarch64")]
impl aster_console::AnyConsoleDevice for SerialConsole {
    fn send(&self, buf: &[u8]) {
        for &byte in buf {
            serial::send(byte);
        }
    }

    fn register_callback(&self, callback: &'static aster_console::ConsoleCallback) {
        UART_CALLBACK.call_once(|| Box::new(callback));
    }
}
