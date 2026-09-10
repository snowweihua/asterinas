// SPDX-License-Identifier: MPL-2.0

//! PCI bus access via ECAM for AArch64.
//!
//! On QEMU `virt` the PCI configuration space is accessed through ECAM (PCIe
//! Enhanced Configuration Access Mechanism) whose MMIO region is described in
//! the device tree under a compatible node like "pci-host-ecam-generic".

use core::ops::RangeInclusive;

use ostd::{arch::boot::DEVICE_TREE, io::IoMem, mm::VmIoOnce, warn, Error};
use spin::Once;

use crate::PciDeviceLocation;

static PCI_ECAM_CFG_SPACE: Once<IoMem> = Once::new();

pub(crate) fn write32(location: &PciDeviceLocation, offset: u32, value: u32) -> Result<(), Error> {
    PCI_ECAM_CFG_SPACE.get().ok_or(Error::IoError)?.write_once(
        (encode_as_address_offset(location) | (offset & 0xfc)) as usize,
        &value,
    )
}

pub(crate) fn read32(location: &PciDeviceLocation, offset: u32) -> Result<u32, Error> {
    PCI_ECAM_CFG_SPACE
        .get()
        .ok_or(Error::IoError)?
        .read_once((encode_as_address_offset(location) | (offset & 0xfc)) as usize)
}

/// Encodes the bus, device, and function into an address offset in the PCI MMIO region.
fn encode_as_address_offset(location: &PciDeviceLocation) -> u32 {
    ((location.bus as u32) << 20)
        | ((location.device as u32) << 15)
        | ((location.function as u32) << 12)
}

pub(crate) fn has_pci_bus() -> bool {
    PCI_ECAM_CFG_SPACE.is_completed()
}

pub(crate) fn init() -> Option<RangeInclusive<u8>> {
    let Some(fdt) = DEVICE_TREE.get() else {
        warn!("Device tree not available, skipping PCI ECAM initialization");
        return None;
    };
    let Some(pci) = fdt.find_compatible(&["pci-host-ecam-generic"]) else {
        warn!("No generic PCI host controller node found in the device tree");
        return None;
    };

    let Some(mut reg) = pci.reg() else {
        warn!("PCI node should have exactly one `reg` property, but found zero `reg`s");
        return None;
    };
    let Some(region) = reg.next() else {
        warn!("PCI node should have exactly one `reg` property, but found zero `reg`s");
        return None;
    };
    if reg.next().is_some() {
        warn!(
            "PCI node should have exactly one `reg` property, but found {} `reg`s",
            reg.count() + 2
        );
        return None;
    }

    let bus_range = if let Some(prop) = pci.property("bus-range") {
        if prop.value.len() != 8 || prop.value[0..3] != [0, 0, 0] || prop.value[4..7] != [0, 0, 0] {
            warn!(
                "node should have a `bus-range` property with two bytes, but found `{:?}`",
                prop.value
            );
            return None;
        }
        if prop.value[3] != 0 {
            warn!(
                "node with a non-zero bus start `{}` is not supported yet",
                prop.value[3]
            );
            return None;
        }
        Some(prop.value[3]..=prop.value[7])
    } else {
        Some(0..=255)
    };

    let addr_start = region.starting_address as usize;
    let addr_end = addr_start.checked_add(region.size.unwrap()).unwrap();
    PCI_ECAM_CFG_SPACE.call_once(|| {
        let mem = IoMem::acquire(addr_start..addr_end).unwrap();
        mem
    });

    bus_range
}

// GICv3 ITS (Interrupt Translation Service) base on QEMU virt for MSI-X.
// This is a reasonable default; real initialization should parse the device tree.
pub(crate) const MSIX_DEFAULT_MSG_ADDR: u32 = 0x0800_0100;

pub(crate) fn construct_remappable_msix_address(_remapping_index: u32) -> u32 {
    unimplemented!()
}
