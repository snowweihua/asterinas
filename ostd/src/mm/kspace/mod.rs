// SPDX-License-Identifier: MPL-2.0

//! Kernel memory space management.
//!
//! The kernel memory space is currently managed as follows, if the
//! address width is 48 bits (with 47 bits kernel space).
//!
//! TODO: the cap of linear mapping (the start of vm alloc) are raised
//! to workaround for high IO in TDX. We need actual vm alloc API to have
//! a proper fix.
//!
//! ```text
//! +-+ <- the highest used address (0xffff_ffff_ffff_0000)
//! | |         For the kernel code, 1 GiB.
//! +-+ <- 0xffff_ffff_8000_0000
//! | |
//! | |         Unused hole.
//! +-+ <- 0xffff_e100_0000_0000
//! | |         For frame metadata, 1 TiB.
//! +-+ <- 0xffff_e000_0000_0000
//! | |         For [`KVirtArea`], 32 TiB.
//! +-+ <- the middle of the higher half (0xffff_c000_0000_0000)
//! | |
//! | |
//! | |
//! | |         For linear mappings, 64 TiB.
//! | |         Mapped physical addresses are untracked.
//! | |
//! | |
//! | |
//! +-+ <- the base of high canonical address (0xffff_8000_0000_0000)
//! ```
//!
//! If the address width is (according to [`crate::arch::mm::PagingConsts`])
//! 39 bits or 57 bits, the memory space just adjust proportionally.

#![cfg_attr(target_arch = "loongarch64", expect(unused_imports))]

pub(crate) mod kvirt_area;

use core::ops::Range;

use log::info;
use crate::boot::SimpleOnce as Once;
#[cfg(ktest)]
mod test;

use super::{
    frame::{
        meta::{mapping, AnyFrameMeta, MetaPageMeta},
        Segment,
    },
    page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
    page_table::{PageTable, PageTableConfig},
    Frame, HasSize, Paddr, PagingConstsTrait, Vaddr,
};
use crate::{
    arch::mm::{PageTableEntry, PagingConsts},
    boot::memory_region::MemoryRegionType,
    mm::{page_table::largest_pages, PagingLevel},
    task::disable_preempt,
};

/// The shortest supported address width is 39 bits. And the literal
/// values are written for 48 bits address width. Adjust the values
/// by arithmetic left shift.
const ADDR_WIDTH_SHIFT: isize = PagingConsts::ADDRESS_WIDTH as isize - 48;

/// Start of the kernel address space.
/// This is the _lowest_ address of the x86-64's _high_ canonical addresses.
#[cfg(not(any(target_arch = "loongarch64", target_arch = "aarch64")))]
pub const KERNEL_BASE_VADDR: Vaddr = 0xffff_8000_0000_0000 << ADDR_WIDTH_SHIFT;
#[cfg(target_arch = "aarch64")]
pub const KERNEL_BASE_VADDR: Vaddr = 0xffff_0000_0000_0000;
#[cfg(target_arch = "loongarch64")]
pub const KERNEL_BASE_VADDR: Vaddr = 0x9000_0000_0000_0000 << ADDR_WIDTH_SHIFT;
/// End of the kernel address space (non inclusive).
pub const KERNEL_END_VADDR: Vaddr = 0xffff_ffff_ffff_0000 << ADDR_WIDTH_SHIFT;

/// The kernel code is linear mapped to this address.
///
/// FIXME: This offset should be randomly chosen by the loader or the
/// boot compatibility layer. But we disabled it because OSTD
/// doesn't support relocatable kernel yet.
pub fn kernel_loaded_offset() -> usize {
    KERNEL_CODE_BASE_VADDR
}

#[cfg(target_arch = "x86_64")]
const KERNEL_CODE_BASE_VADDR: usize = 0xffff_ffff_8000_0000 << ADDR_WIDTH_SHIFT;
#[cfg(target_arch = "riscv64")]
const KERNEL_CODE_BASE_VADDR: usize = 0xffff_ffff_0000_0000 << ADDR_WIDTH_SHIFT;
#[cfg(target_arch = "loongarch64")]
const KERNEL_CODE_BASE_VADDR: usize = 0x9000_0000_0000_0000 << ADDR_WIDTH_SHIFT;
#[cfg(target_arch = "aarch64")]
const KERNEL_CODE_BASE_VADDR: usize = 0xffff_0000_0000_0000 << ADDR_WIDTH_SHIFT;

const FRAME_METADATA_CAP_VADDR: Vaddr = 0xffff_e100_0000_0000 << ADDR_WIDTH_SHIFT;
const FRAME_METADATA_BASE_VADDR: Vaddr = 0xffff_e000_0000_0000 << ADDR_WIDTH_SHIFT;
pub(in crate::mm) const FRAME_METADATA_RANGE: Range<Vaddr> =
    FRAME_METADATA_BASE_VADDR..FRAME_METADATA_CAP_VADDR;

const VMALLOC_BASE_VADDR: Vaddr = 0xffff_c000_0000_0000 << ADDR_WIDTH_SHIFT;
pub const VMALLOC_VADDR_RANGE: Range<Vaddr> = VMALLOC_BASE_VADDR..FRAME_METADATA_BASE_VADDR;

/// The base address of the linear mapping of all physical
/// memory in the kernel address space.
#[cfg(not(target_arch = "loongarch64"))]
pub const LINEAR_MAPPING_BASE_VADDR: Vaddr = 0xffff_8000_0000_0000 << ADDR_WIDTH_SHIFT;
#[cfg(target_arch = "loongarch64")]
pub const LINEAR_MAPPING_BASE_VADDR: Vaddr = 0x9000_0000_0000_0000 << ADDR_WIDTH_SHIFT;
pub const LINEAR_MAPPING_VADDR_RANGE: Range<Vaddr> = LINEAR_MAPPING_BASE_VADDR..VMALLOC_BASE_VADDR;

/// Convert physical address to virtual address using offset, only available inside `ostd`
pub fn paddr_to_vaddr(pa: Paddr) -> usize {
    debug_assert!(pa < VMALLOC_BASE_VADDR - LINEAR_MAPPING_BASE_VADDR);
    pa + LINEAR_MAPPING_BASE_VADDR
}

/// The kernel page table instance.
///
/// It manages the kernel mapping of all address spaces by sharing the kernel part. And it
/// is unlikely to be activated.
pub static KERNEL_PAGE_TABLE: Once<PageTable<KernelPtConfig>> = Once::new();

#[derive(Clone, Debug)]
pub(crate) struct KernelPtConfig {}

// We use the first available PTE bit to mark the frame as tracked.
// SAFETY: `item_into_raw` and `item_from_raw` are implemented correctly,
unsafe impl PageTableConfig for KernelPtConfig {
    const TOP_LEVEL_INDEX_RANGE: Range<usize> = 256..512;
    const TOP_LEVEL_CAN_UNMAP: bool = false;

    type E = PageTableEntry;
    type C = PagingConsts;

    type Item = MappedItem;

    fn item_into_raw(item: Self::Item) -> (Paddr, PagingLevel, PageProperty) {
        match item {
            MappedItem::Tracked(frame, mut prop) => {
                debug_assert!(!prop.priv_flags.contains(PrivilegedPageFlags::AVAIL1));
                prop.priv_flags |= PrivilegedPageFlags::AVAIL1;
                let level = frame.map_level();
                let paddr = frame.into_raw();
                (paddr, level, prop)
            }
            MappedItem::Untracked(pa, level, mut prop) => {
                debug_assert!(!prop.priv_flags.contains(PrivilegedPageFlags::AVAIL1));
                prop.priv_flags -= PrivilegedPageFlags::AVAIL1;
                (pa, level, prop)
            }
        }
    }

    unsafe fn item_from_raw(paddr: Paddr, level: PagingLevel, prop: PageProperty) -> Self::Item {
        if prop.priv_flags.contains(PrivilegedPageFlags::AVAIL1) {
            debug_assert_eq!(level, 1);
            // SAFETY: The caller ensures safety.
            let frame = unsafe { Frame::<dyn AnyFrameMeta>::from_raw(paddr) };
            MappedItem::Tracked(frame, prop)
        } else {
            MappedItem::Untracked(paddr, level, prop)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MappedItem {
    Tracked(Frame<dyn AnyFrameMeta>, PageProperty),
    Untracked(Paddr, PagingLevel, PageProperty),
}

/// Initializes the kernel page table.
///
/// This function should be called after:
///  - the page allocator and the heap allocator are initialized;
///  - the memory regions are initialized.
///
/// This function should be called before:
///  - any initializer that modifies the kernel page table.
pub fn init_kernel_page_table(meta_pages: Segment<MetaPageMeta>) {
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.W] start\n"); }

    // Start to initialize the kernel page table.
    let kpt = PageTable::<KernelPtConfig>::new_kernel_page_table();
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.X] after new_kernel_page_table\n"); }
    let preempt_guard = disable_preempt();
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.Y] after disable_preempt\n"); }

    // In LoongArch64, we don't need to do linear mappings for the kernel because of DMW0.
    // On AArch64, the boot page table already has linear mapping entries.
    #[cfg(all(
        not(target_arch = "loongarch64"),
        not(target_arch = "aarch64")
    ))]
    // Do linear mappings for the kernel.
    {
        #[cfg(target_arch = "aarch64")]
        unsafe { crate::arch::boot::pl011_puts(b"[kspace.a] before max_paddr\n"); }
        let max_paddr = crate::mm::frame::max_paddr();
        #[cfg(target_arch = "aarch64")]
        unsafe { crate::arch::boot::pl011_puts(b"[kspace.b] after max_paddr\n"); }
        let from = LINEAR_MAPPING_BASE_VADDR..LINEAR_MAPPING_BASE_VADDR + max_paddr;
        let prop = PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::GLOBAL,
        };
        let mut cursor = kpt.cursor_mut(&preempt_guard, &from).unwrap();
        for (pa, level) in largest_pages::<KernelPtConfig>(from.start, 0, max_paddr) {
            // SAFETY: we are doing the linear mapping for the kernel.
            unsafe { cursor.map(MappedItem::Untracked(pa, level, prop)) }
                .expect("Kernel linear address space is mapped twice");
        }
    }

    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.m0] before metadata VA compute\n"); }
    let start_va = mapping::frame_to_meta::<PagingConsts>(crate::arch::mm::frame_paddr_base());
    #[cfg(target_arch = "aarch64")]
    unsafe {
        crate::arch::boot::pl011_puts(b"[kspace.m1] start_va=");
        crate::arch::boot::pl011_puts_hex(start_va);
        crate::arch::boot::pl011_puts(b"\n");
    }
    let from = start_va..start_va + meta_pages.size();
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.m2] range computed, prop setup\n"); }
    let prop = PageProperty {
        flags: PageFlags::RW,
        cache: CachePolicy::Writeback,
        priv_flags: PrivilegedPageFlags::GLOBAL,
    };
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.m3] before cursor_mut\n"); }
    {
        let mut cursor = kpt.cursor_mut(&preempt_guard, &from).unwrap();
        let pa_range = meta_pages.clone().into_raw();
        for (pa, level) in
            largest_pages::<KernelPtConfig>(from.start, pa_range.start, pa_range.len())
        {
            unsafe { cursor.map(MappedItem::Untracked(pa, level, prop)) }
                .expect("Frame metadata address space is mapped twice");
        }
    }

    // In LoongArch64, we don't need to do linear mappings for the kernel code because of DMW0.
    #[cfg(all(not(target_arch = "loongarch64"), not(target_arch = "aarch64")))]
    // Map for the kernel code itself.
    // TODO: set separated permissions for each segments in the kernel.
    {
        let regions = &crate::boot::boot_info().memory_regions;
        let region = regions
            .iter()
            .find(|r| r.typ() == MemoryRegionType::Kernel)
            .unwrap();
        let offset = kernel_loaded_offset();
        let from = region.base() + offset..region.end() + offset;
        let prop = PageProperty {
            flags: PageFlags::RWX,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::GLOBAL,
        };
        let mut cursor = kpt.cursor_mut(&preempt_guard, &from).unwrap();
        for (pa, level) in largest_pages::<KernelPtConfig>(from.start, region.base(), from.len()) {
            // SAFETY: we are doing the kernel code mapping.
            unsafe { cursor.map(MappedItem::Untracked(pa, level, prop)) }
                .expect("Kernel code mapped twice");
        }
    }
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[kspace.e] skipped (AArch64 uses slot0 copy)\n"); }

    core::mem::forget(meta_pages);

    KERNEL_PAGE_TABLE.call_once(|| kpt);
}

/// Activates the kernel page table.
///
/// # Safety
///
/// This function should only be called once per CPU.
pub unsafe fn activate_kernel_page_table() {
    let kpt = KERNEL_PAGE_TABLE
        .get()
        .expect("The kernel page table is not initialized yet");

    // SAFETY: the kernel page table is initialized properly.
    unsafe {
        kpt.first_activate_unchecked();

        crate::arch::boot::pl011_puts(b"[akt.0] after first_activate\n");

        #[cfg(target_arch = "aarch64")]
        {
            let ttbr1: usize;
            core::arch::asm!("mrs {0}, ttbr1_el1", out(reg) ttbr1, options(nostack, nomem, preserves_flags));
            crate::arch::boot::pl011_puts(b"[akt.0b] TTBR1=");
            crate::arch::boot::pl011_puts_hex(ttbr1);
            crate::arch::boot::pl011_puts(b"\n");
            let kpt_root: usize;
            core::arch::asm!("mrs {0}, ttbr1_el1", out(reg) kpt_root, options(nostack, nomem, preserves_flags));
            // L0[0]: boot_l3pt_high remap (should be MM_TYPE_TABLE to L1 table)
            let l0_0 = (kpt_root as *const usize).read_volatile();
            crate::arch::boot::pl011_puts(b"[akt.0c] L0[0]=");
            crate::arch::boot::pl011_puts_hex(l0_0);
            crate::arch::boot::pl011_puts(b"\n");
            // L0[256]: boot_l3pt_linear remap (should be MM_TYPE_TABLE to L1 table)
            let l0_256 = (kpt_root as *const usize).add(256).read_volatile();
            crate::arch::boot::pl011_puts(b"[akt.0d] L0[256]=");
            crate::arch::boot::pl011_puts_hex(l0_256);
            crate::arch::boot::pl011_puts(b"\n");
            // L0[511]: the highest slot, covers 0xFFFF800000000000+
            let l0_511 = (kpt_root as *const usize).add(511).read_volatile();
            crate::arch::boot::pl011_puts(b"[akt.0e] L0[511]=");
            crate::arch::boot::pl011_puts_hex(l0_511);
            crate::arch::boot::pl011_puts(b"\n");
            // If L0[511] is a TABLE descriptor, walk L1 table
            if (l0_511 & 0x3) == 3 {
                let l1_table_pa = l0_511 & 0x0000_FFFF_FFFF_F000;
                let l1_table_va = l1_table_pa + 0xFFFF_8000_0000_0000;
                let l1_0 = (l1_table_va as *const usize).read_volatile();
                crate::arch::boot::pl011_puts(b"[akt.0f] L1[0](linear)=");
                crate::arch::boot::pl011_puts_hex(l1_0);
                crate::arch::boot::pl011_puts(b"\n");
                // L1 slot 511 covers the crash VA 0xFFFFFFFFC900A8
                let l1_511 = (l1_table_va as *const usize).add(511).read_volatile();
                crate::arch::boot::pl011_puts(b"[akt.0g] L1[511](crash)=");
                crate::arch::boot::pl011_puts_hex(l1_511);
                crate::arch::boot::pl011_puts(b"\n");
                // Also check L1 slot 448 (metadata range)
                let l1_448 = (l1_table_va as *const usize).add(448).read_volatile();
                crate::arch::boot::pl011_puts(b"[akt.0h] L1[448](meta)=");
                crate::arch::boot::pl011_puts_hex(l1_448);
                crate::arch::boot::pl011_puts(b"\n");
            } else {
                crate::arch::boot::pl011_puts(b"[akt.0i] L0[511] invalid (expected!)\n");
            }
        }

        crate::arch::mm::tlb_flush_all_including_global();
        crate::arch::boot::pl011_puts(b"[akt.1] after tlb_flush\n");
    }

    unsafe {
        crate::arch::boot::pl011_puts(b"[akt.2] skipping dismiss\n");
        crate::arch::boot::pl011_puts(b"[akt.3] after dismiss\n");
    }
}
