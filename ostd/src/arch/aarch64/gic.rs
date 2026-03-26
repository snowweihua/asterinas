// SPDX-License-Identifier: MPL-2.0

//! GICv2 support for AArch64 on QEMU `virt`.

use arm_gic::{
    gicv2::{
        registers::{Gicc, Gicd},
        GicV2,
    },
    IntId, Trigger,
};
use spin::{Mutex, Once};

const QEMU_VIRT_GICD_BASE: usize = 0x0800_0000;
const QEMU_VIRT_GICC_BASE: usize = 0x0801_0000;

static GIC: Once<Mutex<GicV2<'static>>> = Once::new();

fn irq_num_to_intid(irq_num: u8) -> IntId {
    match irq_num {
        0..16 => IntId::sgi(irq_num as u32),
        16..32 => IntId::ppi((irq_num - 16) as u32),
        _ => IntId::spi((irq_num - 32) as u32),
    }
}

fn intid_to_irq_num(intid: IntId) -> Option<usize> {
    let raw = u32::from(intid);
    if raw <= u8::MAX as u32 {
        Some(raw as usize)
    } else {
        None
    }
}

pub(crate) unsafe fn init_on_bsp() {
    GIC.call_once(|| {
        let mut gic = unsafe {
            GicV2::new(
                QEMU_VIRT_GICD_BASE as *mut Gicd,
                QEMU_VIRT_GICC_BASE as *mut Gicc,
            )
        };
        gic.setup();
        Mutex::new(gic)
    });
}

pub(crate) fn init_interrupt(irq_num: u8) {
    let gic = GIC.get().expect("GICv2 is not initialized");
    let mut gic = gic.lock();

    let intid = irq_num_to_intid(irq_num);
    if intid.is_spi() {
        gic.set_trigger(intid, Trigger::Level);
    }
    gic.set_interrupt_priority(intid, 0x80);
    gic.enable_interrupt(intid, true)
        .expect("failed to enable GIC interrupt");
}

pub(crate) fn acknowledge_interrupt() -> Option<usize> {
    let gic = GIC.get().expect("GICv2 is not initialized");
    let mut gic = gic.lock();
    gic.get_and_acknowledge_interrupt()
        .and_then(intid_to_irq_num)
}

pub(crate) fn end_interrupt(irq_num: usize) {
    if irq_num > u8::MAX as usize {
        return;
    }

    let gic = GIC.get().expect("GICv2 is not initialized");
    let mut gic = gic.lock();
    gic.end_interrupt(irq_num_to_intid(irq_num as u8));
}
