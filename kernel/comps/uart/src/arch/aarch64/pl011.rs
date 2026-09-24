// SPDX-License-Identifier: MPL-2.0

use alloc::{string::ToString, vec::Vec};

use fdt::node::FdtNode;
use ostd::{arch::boot::DEVICE_TREE, io::IoMem, irq::IrqLine, mm::VmIoOnce, sync::SpinLock};
use spin::Once;

use crate::console::{Uart, UartConsole};

const OFFSET_UARTDR: usize = 0x000;
const OFFSET_UARTFR: usize = 0x018;
const OFFSET_UARTIMSC: usize = 0x038;
const OFFSET_UARTICR: usize = 0x044;

const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;
const INT_RXIM: u32 = 1 << 4;

struct Pl011 {
    io_mem: IoMem,
}

impl Pl011 {
    fn read_reg(&self, offset: usize) -> u32 {
        self.io_mem.read_once(offset).unwrap()
    }

    fn write_reg(&self, offset: usize, val: u32) {
        self.io_mem.write_once(offset, &val).unwrap();
    }
}

impl Uart for SpinLock<Pl011, ostd::sync::LocalIrqDisabled> {
    fn send(&self, buf: &[u8]) {
        let uart = self.lock();
        for byte in buf {
            while uart.read_reg(OFFSET_UARTFR) & FR_TXFF != 0 {
                core::hint::spin_loop();
            }
            uart.write_reg(OFFSET_UARTDR, *byte as u32);
            // RPi3: pace user-space console output so the lossy USB serial
            // relay does not drop the burst (raw text bursts get lost; paced
            // output survives).
            #[cfg(target_arch = "aarch64")]
            if ostd::arch::is_rpi3() {
                ostd::arch::serial::spin_delay_ms(2);
            }
        }
    }

    fn recv(&self, buf: &mut [u8]) -> usize {
        let uart = self.lock();
        let mut count = 0;
        for slot in buf.iter_mut() {
            if uart.read_reg(OFFSET_UARTFR) & FR_RXFE != 0 {
                break;
            }
            *slot = uart.read_reg(OFFSET_UARTDR) as u8;
            count += 1;
        }
        count
    }

    fn flush(&self) {
        let uart = self.lock();
        // RPi3 polls RX (no IRQ): keep the mask clear so error bits cannot
        // raise a GPU IRQ storm; QEMU keeps the IRQ path.
        if !ostd::arch::is_rpi3() {
            uart.write_reg(OFFSET_UARTIMSC, INT_RXIM);
        }
        uart.write_reg(OFFSET_UARTICR, 0x7FF);
    }
}

static IRQ_LINE: Once<IrqLine> = Once::new();

fn cells_to_usize(bytes: &[u8]) -> usize {
    let mut val = 0usize;
    for chunk in bytes.chunks_exact(4) {
        val = (val << 32) | u32::from_be_bytes(chunk.try_into().unwrap()) as usize;
    }
    val
}

fn cells_prop(node: &FdtNode, name: &str, default: usize) -> usize {
    node.property(name)
        .and_then(|prop| prop.as_usize())
        .unwrap_or(default)
}

/// Translates a `/soc` child bus address into a CPU physical address.
///
/// Devices under `/soc` (e.g. `0x7e201000`) live on a bus segment described
/// by `/soc/ranges`; without translation the driver would map the wrong page.
/// Trees without `/soc/ranges` (e.g. QEMU `virt`) use identity addresses.
fn translate_soc_address(addr: usize) -> usize {
    let Some(fdt) = DEVICE_TREE.get() else {
        return addr;
    };
    let Some(soc) = fdt.find_node("/soc") else {
        return addr;
    };
    let Some(ranges) = soc.property("ranges") else {
        return addr;
    };
    let child_cells = cells_prop(&soc, "#address-cells", 2);
    let size_cells = cells_prop(&soc, "#size-cells", 1);
    let parent_cells = fdt
        .root()
        .property("#address-cells")
        .and_then(|prop| prop.as_usize())
        .unwrap_or(2);
    let stride = (child_cells + parent_cells + size_cells) * 4;
    if stride == 0 {
        return addr;
    }
    for entry in ranges.value.chunks_exact(stride) {
        let child_base = cells_to_usize(&entry[..child_cells * 4]);
        let parent_base =
            cells_to_usize(&entry[child_cells * 4..(child_cells + parent_cells) * 4]);
        let size = cells_to_usize(&entry[(child_cells + parent_cells) * 4..]);
        if child_base <= addr && addr < child_base + size {
            return parent_base + (addr - child_base);
        }
    }
    addr
}

fn interrupt_id(node: &FdtNode) -> Option<u8> {
    let prop = node.property("interrupts")?;
    let cells: Vec<u32> = prop
        .value
        .chunks_exact(4)
        .map(|c| u32::from_be_bytes(c.try_into().unwrap()))
        .collect();
    let (kind, num) = match cells.as_slice() {
        [kind, num, ..] => (*kind, *num),
        [num] => (0, *num),
        _ => return None,
    };
    let intid = match kind {
        0 | 2 => 32 + num,
        1 => 16 + num,
        _ => return None,
    };
    u8::try_from(intid).ok()
}

pub(super) fn init(node: FdtNode) {
    let Some(reg) = node.reg().and_then(|mut regs| regs.next()) else {
        ostd::info!("Failed to read 'reg' property from PL011 node");
        return;
    };
    let Some(reg_size) = reg.size else {
        ostd::info!("Incomplete 'reg' property found in PL011 node");
        return;
    };

    let reg_addr = translate_soc_address(reg.starting_address as usize);
    let Ok(io_mem) = IoMem::acquire(reg_addr..reg_addr + reg_size) else {
        ostd::info!("I/O memory is not available for PL011");
        return;
    };

    let Some(intid) = interrupt_id(&node) else {
        ostd::info!("Failed to read 'interrupts' property from PL011 node");
        return;
    };

    let Ok(mut irq_line) = IrqLine::alloc_specific(intid) else {
        ostd::info!("IRQ line is not available for PL011");
        return;
    };

    let uart_console = UartConsole::new(SpinLock::new(Pl011 { io_mem }));

    aster_console::register_device(
        aster_console::UART_CONSOLE_NAME.to_string(),
        uart_console.clone(),
    );

    let cloned_uart_console = uart_console.clone();
    irq_line.on_active(move |_| cloned_uart_console.trigger_input_callbacks());
    IRQ_LINE.call_once(move || irq_line);
    uart_console.uart().flush();

    ostd::info!("Registered PL011 as a console");
}
