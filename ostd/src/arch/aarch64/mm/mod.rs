// SPDX-License-Identifier: MPL-2.0

//! AArch64 memory management: boot and runtime page tables plus
//! fallible user-memory helpers.
//!
//! # Safety
//!
//! Page-table memory is managed by the frame allocator with the correct
//! page-table page type. User-memory helpers (`memcpy_fallible.S`) run
//! under the exception table, so a fault on the accessed range returns
//! failure instead of unwinding into the kernel.

use alloc::fmt;
use core::{arch::asm, ops::Range};

core::arch::global_asm!(include_str!("memcpy_fallible.S"));

unsafe extern "C" {
    pub(crate) fn __memcpy_fallible(dst: *mut u8, src: *const u8, size: usize) -> usize;
    pub(crate) fn __memset_fallible(dst: *mut u8, value: u8, size: usize) -> usize;
    pub(crate) fn __atomic_load_fallible(ptr: *const u32) -> u64;
    pub(crate) fn __atomic_cmpxchg_fallible(
        ptr: *mut u32,
        old_val: u32,
        new_val: u32,
    ) -> u64;
}

use crate::{
    mm::{
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags as PrivFlags},
        page_table::PageTableEntryTrait,
        Paddr, PagingConstsTrait, PagingLevel, PodOnce, Vaddr, PAGE_SIZE,
    },
    Pod,
};

pub(crate) const NR_ENTRIES_PER_PAGE: usize = 512;

pub(crate) fn frame_paddr_base() -> usize {
    crate::arch::board::dram_base()
}

#[derive(Clone, Debug, Default)]
pub struct PagingConsts {}

impl PagingConstsTrait for PagingConsts {
    const BASE_PAGE_SIZE: usize = 4096;
    const NR_LEVELS: PagingLevel = 4;
    const ADDRESS_WIDTH: usize = 48;
    const VA_SIGN_EXT: bool = true;
    const HIGHEST_TRANSLATION_LEVEL: PagingLevel = 1;
    const PTE_SIZE: usize = size_of::<PageTableEntry>();
}

bitflags::bitflags! {
    #[derive(Pod)]
    #[repr(C)]
    pub struct PageTableFlags: usize {
        const VALID = 1 << 0;
        const TYPE = 1 << 1;
        const ATTRINDX0 = 1 << 2;
        const ATTRINDX1 = 1 << 3;
        const ATTRINDX2 = 1 << 4;
        const EL1_EL0 = 1 << 6;
        const WRITABLE = 1 << 7;
        const SHAREABLE = 1 << 8;
        const ACCESSED = 1 << 10;
        /// Not-Global (nG): if set, the TLB entry is ASID-tagged (non-global).
        /// User-space pages must have this set so that TLBI by VA (vaae1/vae1)
        /// correctly targets them. Without nG=1, user pages are treated as global
        /// and `tlbi vaae1` will not flush them.
        const NOT_GLOBAL = 1 << 11;
        const RSV1 = 1 << 55;
        const RSV2 = 1 << 56;
        const DIRTY = 1 << 51;
        const PXN = 1 << 53;
        const UXN = 1 << 54;
    }
}

const ATTRINDX_MASK: usize = 0b111 << 2;
const ATTRINDX_WRITEBACK: usize = 0b001 << 2;
const ATTRINDX_UNCACHEABLE: usize = 0b010 << 2;
const SH_MASK: usize = 0b11 << 8;
const SH_INNER_SHAREABLE: usize = 0b11 << 8;

pub(crate) fn tlb_flush_addr(vaddr: Vaddr) {
    if crate::arch::board::IS_HARDWARE.load(core::sync::atomic::Ordering::Relaxed) {
        unsafe {
            asm!("dsb ishst", options(nostack, nomem, preserves_flags));
            asm!("tlbi vaae1, {0}", in(reg) vaddr, options(nostack, nomem, preserves_flags));
            asm!("dsb ish", options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    } else {
        // WORKAROUND: QEMU 6.2 hangs on ANY TLBI instruction (`vaae1`, `vmalle1`,
        // etc.) once any user-space PTE has been written (COW, stack init, etc.).
        // On single-CPU QEMU TCG, the software TLB self-invalidates on the next
        // page-table walk triggered by the subsequent access fault; explicit TLBI
        // is not required for correctness in this uniprocessor configuration.
        unsafe {
            asm!("dsb ishst", options(nostack, nomem, preserves_flags));
            asm!("dsb ish", options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    }
}

pub(crate) fn tlb_flush_addr_range(range: &Range<Vaddr>) {
    for vaddr in range.clone().step_by(PAGE_SIZE) {
        tlb_flush_addr(vaddr);
    }
}

pub(crate) fn tlb_flush_all_excluding_global() {
    if crate::arch::board::IS_HARDWARE.load(core::sync::atomic::Ordering::Relaxed) {
        unsafe {
            asm!("dsb ishst", options(nostack, nomem, preserves_flags));
            asm!("tlbi vmalle1", options(nostack, nomem, preserves_flags));
            asm!("dsb ish", options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    } else {
        // WORKAROUND: QEMU 6.2 AArch64 TCG deadlocks on `tlbi vmalle1` when called
        // after user-space PTEs have been written. Instead, trigger QEMU's soft-TLB
        // flush by writing TTBR0_EL1 to itself (any write causes tlb_flush_by_mmuidx).
        // This clears negative/stale TLB entries without corrupting the page walk state.
        unsafe {
            let ttbr0: u64;
            asm!("mrs {0}, ttbr0_el1", out(reg) ttbr0, options(nostack, nomem, preserves_flags));
            asm!("dsb sy", options(nostack, nomem, preserves_flags));
            asm!("msr ttbr0_el1, {0}", in(reg) ttbr0, options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    }
}

pub(crate) fn tlb_flush_all_including_global() {
    if crate::arch::board::IS_HARDWARE.load(core::sync::atomic::Ordering::Relaxed) {
        unsafe {
            asm!("dsb ishst", options(nostack, nomem, preserves_flags));
            asm!("tlbi vmalle1", options(nostack, nomem, preserves_flags));
            asm!("dsb ish", options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    } else {
        // Same QEMU workaround as `tlb_flush_all_excluding_global`.
        unsafe {
            let ttbr0: u64;
            asm!("mrs {0}, ttbr0_el1", out(reg) ttbr0, options(nostack, nomem, preserves_flags));
            asm!("dsb sy", options(nostack, nomem, preserves_flags));
            asm!("msr ttbr0_el1, {0}", in(reg) ttbr0, options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    }
}

pub unsafe fn activate_page_table(root_paddr: Paddr, _root_pt_cache: CachePolicy) {
    assert!(root_paddr % PagingConsts::BASE_PAGE_SIZE == 0);
    // On AArch64, the kernel page table covers the high VA half and is loaded into TTBR1_EL1.
    unsafe {
        asm!("dsb sy", options(nostack, nomem, preserves_flags));
        asm!("msr ttbr1_el1, {0}", in(reg) root_paddr, options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
    // On real hardware, invalidate all TLB entries so the new kernel mappings take effect.
    if crate::arch::board::IS_HARDWARE.load(core::sync::atomic::Ordering::Relaxed) {
        unsafe {
            asm!("tlbi vmalle1", options(nostack, nomem, preserves_flags));
            asm!("dsb ish", options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    }
}

/// Activates the user (low-VA) page table by loading it into TTBR0_EL1.
///
/// # Safety
///
/// The caller must ensure that the page table is valid and covers the user address space.
pub unsafe fn activate_user_page_table(root_paddr: Paddr) {
    assert!(root_paddr % PagingConsts::BASE_PAGE_SIZE == 0);
    unsafe {
        asm!("dsb sy", options(nostack, nomem, preserves_flags));
        asm!("msr ttbr0_el1, {0}", in(reg) root_paddr, options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
    // On real hardware we must flush stale user TLB entries when switching page tables.
    if crate::arch::board::IS_HARDWARE.load(core::sync::atomic::Ordering::Relaxed) {
        unsafe {
            asm!("tlbi vmalle1", options(nostack, nomem, preserves_flags));
            asm!("dsb ish", options(nostack, nomem, preserves_flags));
            asm!("isb", options(nostack, nomem, preserves_flags));
        }
    }
}

/// Returns the physical address of the currently active user page table (TTBR0_EL1).
pub fn current_user_page_table_paddr() -> Paddr {
    let root_paddr: Paddr;
    unsafe {
        asm!("mrs {0}, ttbr0_el1", out(reg) root_paddr, options(nostack, nomem, preserves_flags));
    }
    root_paddr
}

pub fn current_page_table_paddr() -> Paddr {
    let root_paddr: Paddr;
    unsafe {
        asm!("mrs {0}, ttbr1_el1", out(reg) root_paddr, options(nostack, nomem, preserves_flags));
    }
    root_paddr
}

#[derive(Clone, Copy, Pod, Default)]
#[repr(C)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    const PHYS_ADDR_MASK: usize = 0x0000_FFFF_FFFF_F000;
}

macro_rules! parse_flags {
    ($val:expr, $from:expr, $to:expr) => {
        ($val as usize & $from.bits() as usize) >> $from.bits().ilog2() << $to.bits().ilog2()
    };
}

impl PodOnce for PageTableEntry {}

impl PageTableEntryTrait for PageTableEntry {
    fn is_present(&self) -> bool {
        self.0 & PageTableFlags::VALID.bits() != 0
    }

    fn new_page(paddr: Paddr, level: PagingLevel, prop: PageProperty) -> Self {
        let mut pte = Self(paddr & Self::PHYS_ADDR_MASK);
        pte.set_prop(prop);
        // On AArch64, level-1 (L3 leaf) page descriptors require TYPE bit (bit 1) set.
        // Without it, the PTE is treated as invalid by the hardware.
        if level == 1 {
            pte.0 |= PageTableFlags::TYPE.bits();
        }
        pte
    }

    fn new_pt(paddr: Paddr) -> Self {
        Self(
            paddr & Self::PHYS_ADDR_MASK
                | PageTableFlags::VALID.bits()
                | PageTableFlags::TYPE.bits(),
        )
    }

    fn paddr(&self) -> Paddr {
        self.0 & Self::PHYS_ADDR_MASK
    }

    fn prop(&self) -> PageProperty {
        let mut flags = parse_flags!(!(self.0), PageTableFlags::PXN, PageFlags::X)
            | parse_flags!(self.0, PageTableFlags::ACCESSED, PageFlags::ACCESSED)
            | parse_flags!(self.0, PageTableFlags::DIRTY, PageFlags::DIRTY)
            | parse_flags!(self.0, PageTableFlags::RSV2, PageFlags::AVAIL2);
        let mut priv_flags = parse_flags!(self.0, PageTableFlags::RSV1, PrivFlags::AVAIL1);

        flags |= PageFlags::R.bits() as usize;
        // AP[2] (WRITABLE, bit 7) = 0 means read/write; = 1 means read-only.
        if self.0 & PageTableFlags::WRITABLE.bits() == 0 {
            flags |= PageFlags::W.bits() as usize;
        }
        if self.0 & PageTableFlags::EL1_EL0.bits() != 0 {
            priv_flags |= PrivFlags::USER.bits() as usize;
        }

        let cache = match self.0 & ATTRINDX_MASK {
            ATTRINDX_UNCACHEABLE => CachePolicy::Uncacheable,
            _ => CachePolicy::Writeback,
        };

        PageProperty {
            flags: PageFlags::from_bits(flags as u8).unwrap(),
            cache,
            priv_flags: PrivFlags::from_bits(priv_flags as u8).unwrap(),
        }
    }

    #[expect(clippy::precedence)]
    fn set_prop(&mut self, prop: PageProperty) {
        // AP[2] (bit 7, named WRITABLE) = 0 means read/write; = 1 means read-only.
        // So when PageFlags::W is set, we must NOT set WRITABLE (AP[2]=0 = writable).
        // When W is NOT set, we set WRITABLE (AP[2]=1 = read-only).
        let mut flags = PageTableFlags::VALID.bits()
            | PageTableFlags::ACCESSED.bits() // AArch64 requires AF=1 to avoid access flag fault
            | parse_flags!(!prop.flags.bits(), PageFlags::X, PageTableFlags::PXN)
            | parse_flags!(!prop.flags.bits(), PageFlags::W, PageTableFlags::WRITABLE)
            | parse_flags!(prop.flags.bits(), PageFlags::DIRTY, PageTableFlags::DIRTY)
            | parse_flags!(prop.flags.bits(), PageFlags::ACCESSED, PageTableFlags::ACCESSED)
            | parse_flags!(prop.flags.bits(), PageFlags::AVAIL2, PageTableFlags::RSV2)
            | parse_flags!(prop.priv_flags.bits(), PrivFlags::AVAIL1, PageTableFlags::RSV1);

        if prop.priv_flags.contains(PrivFlags::USER) {
            flags |= PageTableFlags::EL1_EL0.bits();
            // User-space pages must be marked as non-global (nG=1) so that TLB
            // entries are ASID-tagged. Without this, `tlbi vaae1` cannot flush
            // user-space TLB entries as it only targets non-global entries.
            flags |= PageTableFlags::NOT_GLOBAL.bits();
        }

        flags |= match prop.cache {
            CachePolicy::Uncacheable => ATTRINDX_UNCACHEABLE,
            CachePolicy::Writeback | CachePolicy::Writethrough => ATTRINDX_WRITEBACK,
            CachePolicy::WriteCombining | CachePolicy::WriteProtected => ATTRINDX_UNCACHEABLE,
        };
        flags |= SH_INNER_SHAREABLE;

        // IMPORTANT: Preserve the TYPE bit (bit 1) if it was already set.
        // The TYPE bit is NOT part of PageProperty but is a structural AArch64
        // descriptor field:
        //   - Set in new_page() for level-1 (leaf) descriptors (required for valid
        //     L3 page descriptors: bits[1:0] must be 0b11, else hardware treats as INVALID)
        //   - Set in new_pt() for table descriptors at all levels
        // Without preservation, protect_next() → set_prop() would clear the TYPE bit,
        // making the leaf PTE bits[1:0] = 0b01 which the hardware page walker treats
        // as an invalid/reserved descriptor → spurious translation-fault-L3 faults.
        let old_type = self.0 & PageTableFlags::TYPE.bits();
        self.0 = (self.0 & Self::PHYS_ADDR_MASK) | flags | old_type;
        self.0 &= !SH_MASK;
        self.0 |= SH_INNER_SHAREABLE;
    }

    fn is_last(&self, level: PagingLevel) -> bool {
        level == 1 || self.0 & PageTableFlags::TYPE.bits() == 0
    }
}

impl fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut f = f.debug_struct("PageTableEntry");
        f.field("raw", &format_args!("{:#x}", self.0))
            .field("paddr", &format_args!("{:#x}", self.paddr()))
            .field("present", &self.is_present())
            .field(
                "flags",
                &PageTableFlags::from_bits_truncate(self.0 & !Self::PHYS_ADDR_MASK),
            )
            .field("prop", &self.prop())
            .finish()
    }
}

