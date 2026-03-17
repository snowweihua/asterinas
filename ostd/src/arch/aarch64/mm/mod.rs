// SPDX-License-Identifier: MPL-2.0

use alloc::fmt;
use core::{arch::asm, ops::Range};

use crate::{
    mm::{
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags as PrivFlags},
        page_table::PageTableEntryTrait,
        Paddr, PagingConstsTrait, PagingLevel, PodOnce, Vaddr, PAGE_SIZE,
    },
    Pod,
};

pub(crate) const NR_ENTRIES_PER_PAGE: usize = 512;

pub(crate) const fn frame_paddr_base() -> usize {
    0x4000_0000
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
    unsafe {
        asm!("dsb ishst", options(nostack, nomem, preserves_flags));
        asm!("tlbi vae1is, {0}", in(reg) vaddr, options(nostack, nomem, preserves_flags));
        asm!("dsb ish", options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
}

pub(crate) fn tlb_flush_addr_range(range: &Range<Vaddr>) {
    for vaddr in range.clone().step_by(PAGE_SIZE) {
        tlb_flush_addr(vaddr);
    }
}

pub(crate) fn tlb_flush_all_excluding_global() {
    unsafe {
        asm!("dsb ishst", options(nostack, nomem, preserves_flags));
        // `aside1is` requires an ASID operand in a register. Use `vmalle1is`
        // which invalidates all EL1 TLB entries (including global) for now.
        // TODO: implement proper ASID-based flushing when ASID support is added.
        asm!("tlbi vmalle1is", options(nostack, nomem, preserves_flags));
        asm!("dsb ish", options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
}

pub(crate) fn tlb_flush_all_including_global() {
    unsafe {
        asm!("dsb ishst", options(nostack, nomem, preserves_flags));
        asm!("tlbi alle1is", options(nostack, nomem, preserves_flags));
        asm!("dsb ish", options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
}

pub unsafe fn activate_page_table(root_paddr: Paddr, _root_pt_cache: CachePolicy) {
    assert!(root_paddr % PagingConsts::BASE_PAGE_SIZE == 0);
    unsafe {
        asm!("dsb ishst", options(nostack, nomem, preserves_flags));
        asm!("msr ttbr0_el1, {0}", in(reg) root_paddr, options(nostack, nomem, preserves_flags));
        asm!("tlbi alle1is", options(nostack, nomem, preserves_flags));
        asm!("dsb ish", options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
}

pub fn current_page_table_paddr() -> Paddr {
    let root_paddr: Paddr;
    unsafe {
        asm!("mrs {0}, ttbr0_el1", out(reg) root_paddr, options(nostack, nomem, preserves_flags));
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

    fn new_page(paddr: Paddr, _level: PagingLevel, prop: PageProperty) -> Self {
        let mut pte = Self(paddr & Self::PHYS_ADDR_MASK);
        pte.set_prop(prop);
        pte
    }

    fn new_pt(paddr: Paddr) -> Self {
        Self(paddr & Self::PHYS_ADDR_MASK | PageTableFlags::VALID.bits() | PageTableFlags::TYPE.bits())
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
        if self.0 & PageTableFlags::WRITABLE.bits() != 0 {
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
        let mut flags = PageTableFlags::VALID.bits()
            | parse_flags!(!prop.flags.bits(), PageFlags::X, PageTableFlags::PXN)
            | parse_flags!(prop.flags.bits(), PageFlags::W, PageTableFlags::WRITABLE)
            | parse_flags!(prop.flags.bits(), PageFlags::DIRTY, PageTableFlags::DIRTY)
            | parse_flags!(prop.flags.bits(), PageFlags::ACCESSED, PageTableFlags::ACCESSED)
            | parse_flags!(prop.flags.bits(), PageFlags::AVAIL2, PageTableFlags::RSV2)
            | parse_flags!(prop.priv_flags.bits(), PrivFlags::AVAIL1, PageTableFlags::RSV1);

        if prop.priv_flags.contains(PrivFlags::USER) {
            flags |= PageTableFlags::EL1_EL0.bits();
        }

        flags |= match prop.cache {
            CachePolicy::Uncacheable => ATTRINDX_UNCACHEABLE,
            CachePolicy::Writeback | CachePolicy::Writethrough => ATTRINDX_WRITEBACK,
            CachePolicy::WriteCombining | CachePolicy::WriteProtected => ATTRINDX_UNCACHEABLE,
        };
        flags |= SH_INNER_SHAREABLE;

        self.0 = (self.0 & Self::PHYS_ADDR_MASK) | flags;
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

pub(crate) unsafe fn __memcpy_fallible(dst: *mut u8, src: *const u8, size: usize) -> usize {
    unsafe { core::ptr::copy(src, dst, size) };
    0
}

pub(crate) unsafe fn __memset_fallible(dst: *mut u8, value: u8, size: usize) -> usize {
    unsafe { core::ptr::write_bytes(dst, value, size) };
    0
}

pub(crate) unsafe fn __atomic_load_fallible(ptr: *const u32) -> u64 {
    let atomic_ptr = ptr.cast::<core::sync::atomic::AtomicU32>();
    unsafe { (*atomic_ptr).load(core::sync::atomic::Ordering::Relaxed) as u64 }
}

pub(crate) unsafe fn __atomic_cmpxchg_fallible(ptr: *mut u32, old_val: u32, new_val: u32) -> u64 {
    let atomic_ptr = ptr.cast::<core::sync::atomic::AtomicU32>();
    let old = unsafe {
        (*atomic_ptr)
            .compare_exchange(
                old_val,
                new_val,
                core::sync::atomic::Ordering::Relaxed,
                core::sync::atomic::Ordering::Relaxed,
            )
            .unwrap_or_else(|actual| actual)
    };
    old as u64
}
