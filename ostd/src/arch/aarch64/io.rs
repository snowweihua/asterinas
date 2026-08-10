// SPDX-License-Identifier: MPL-2.0

use alloc::vec::Vec;

use crate::{boot::memory_region::MemoryRegionType, io::IoMemAllocatorBuilder};

/// Initializes the allocatable MMIO area based on the RISC-V memory
/// distribution map.
///
/// Here we consider all the holes (filtering usable RAM) in the physical
/// address space as MMIO regions.
pub(super) fn construct_io_mem_allocator_builder() -> IoMemAllocatorBuilder {
    let _regions = &crate::boot::EARLY_INFO.get().unwrap().memory_regions;

    let ranges = Vec::new();

    // SAFETY: The range is guaranteed not to access physical memory.
    unsafe { IoMemAllocatorBuilder::new(ranges) }
}
