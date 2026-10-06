// SPDX-License-Identifier: MPL-2.0

pub(super) use ostd::irq::IrqLine as MappedIrqLine;

use ostd::arch::boot::DEVICE_TREE;

pub(super) fn probe_for_device() {
    // The device tree parsing logic here assumed a Linux-compatible device
    // tree.
    // Reference: <https://www.kernel.org/doc/Documentation/devicetree/bindings/virtio/mmio.txt>.
    let device_tree = DEVICE_TREE.get().unwrap();
    let mmio_nodes = device_tree.all_nodes().filter(|node| {
        node.compatible().is_some_and(|compatibles| {
            compatibles
                .all()
                .any(|compatible| compatible == "virtio,mmio")
        })
    });
    mmio_nodes.for_each(|node| {
        let mmio_region = node.reg().unwrap().next().unwrap();
        let mmio_start = mmio_region.starting_address as usize;
        let mmio_end = mmio_start + mmio_region.size.unwrap();

        let mut cells = node.interrupts().unwrap();
        let first = cells.next().unwrap() as u32;
        let intid = if first <= 1 {
            let second = cells.next().unwrap() as u32;
            if first == 0 {
                32 + second
            } else {
                16 + second
            }
        } else {
            first
        } as u8;

        let _ = super::try_register_mmio_device(mmio_start..mmio_end, |irq_line| {
            drop(irq_line);
            let specific = ostd::irq::IrqLine::alloc_specific(intid)?;
            Ok(specific)
        });
    });
}
