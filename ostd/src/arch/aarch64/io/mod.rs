// SPDX-License-Identifier: MPL-2.0

use alloc::vec::Vec;

use crate::{boot::memory_region::MemoryRegionType, io::IoMemAllocatorBuilder};

pub(crate) mod io_mem;

pub const MAX_IO_PORT: u16 = 0;

/// Initializes the allocatable MMIO area from the boot memory regions.
///
/// All address ranges that are not backed by usable physical memory are
/// considered MMIO regions available for allocation.
///
/// # Safety
///
/// The caller ensures that the kernel page table is already activated and
/// that this builder is created only once.
pub(super) unsafe fn construct_io_mem_allocator_builder() -> IoMemAllocatorBuilder {
    let regions = &crate::boot::EARLY_INFO.get().unwrap().memory_regions;
    // NonVolatileSleep marks MMIO windows that must stay linearly mapped;
    // they are excluded from "memory" here so they land in the MMIO pool.
    let reserved_filter = regions.iter().filter(|r| {
        r.typ() != MemoryRegionType::Unknown
            && r.typ() != MemoryRegionType::Reserved
            && r.typ() != MemoryRegionType::Framebuffer
            && r.typ() != MemoryRegionType::NonVolatileSleep
    });

    let mut ranges = Vec::new();

    let mut current_address = 0;
    for region in reserved_filter {
        if current_address < region.base() {
            ranges.push(current_address..region.base());
        }
        current_address = region.end();
    }
    if current_address < usize::MAX {
        ranges.push(current_address..usize::MAX);
    }

    // SAFETY: This is the only place that creates an `IoMemAllocatorBuilder`.
    unsafe { IoMemAllocatorBuilder::new(ranges) }
}
