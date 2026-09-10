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

#[inline]
fn gicc_base_va() -> usize {
    crate::mm::kspace::paddr_to_vaddr(QEMU_VIRT_GICC_BASE)
}

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
    if crate::arch::board::BoardType::cached() != 2 {
        GIC.call_once(|| {
            let mut gic = unsafe {
                GicV2::new(
                    QEMU_VIRT_GICD_BASE as *mut Gicd,
                    QEMU_VIRT_GICC_BASE as *mut Gicc,
                )
            };
            gic.setup();
            // Enable forwarding for both Group 0 and Group 1; the arm-gic
            // setup only sets Group 1.  Also set AckCtl so GICC_IAR can
            // acknowledge either group.
            unsafe {
                core::ptr::write_volatile(
                    (QEMU_VIRT_GICD_BASE) as *mut u32,
                    0x3,
                );
                core::ptr::write_volatile(
                    (QEMU_VIRT_GICC_BASE) as *mut u32,
                    0b111,
                );
            }
            Mutex::new(gic)
        });
    } else {
    }
}

pub(crate) fn init_interrupt(irq_num: u8) {
    if crate::arch::board::BoardType::cached() == 2 {
        // The Raspberry Pi 3 has no GIC; enable the per-IP interrupt router.
        if irq_num == 30 {
            crate::arch::bcm2836_irq::enable_cntpns_irq();
        } else if irq_num == 27 {
            crate::arch::bcm2836_irq::enable_cntv_irq();
        } else if irq_num == crate::arch::bcm2836_irq::MINIUART_IRQ_NUM as u8 {
            crate::arch::bcm2836_irq::enable_miniuart_irq();
        } else if irq_num == 57 {
            crate::arch::bcm2836_irq::enable_uart_irq();
        }
        return;
    }
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
    if crate::arch::board::BoardType::cached() == 2 {
        // The Raspberry Pi 3 has no GIC; read the ARM-local IRQ source instead.
        let irq = crate::arch::bcm2836_irq::acknowledge_interrupt();
        return (irq != 0).then_some(irq);
    }
    // Use the banked GICC_IAR so we can acknowledge both Group 0 and Group 1
    // interrupts (the arm-gic crate only reads GICC_AIAR, which is Group 1).
    let raw = unsafe { core::ptr::read_volatile((gicc_base_va() + 0x0c) as *const u32) };
    if raw == 1023 {
        None
    } else {
        Some(raw as usize)
    }
}

pub(crate) fn end_interrupt(irq_num: usize) {
    if crate::arch::board::BoardType::cached() == 2 {
        crate::arch::bcm2836_irq::end_interrupt(irq_num);
        return;
    }
    if irq_num > u16::MAX as usize {
        return;
    }

    // Drop the GICC_IAR pending priority by writing GICC_EOIR.
    unsafe {
        core::ptr::write_volatile(
            (gicc_base_va() + 0x10) as *mut u32,
            irq_num as u32,
        );
    }
}
