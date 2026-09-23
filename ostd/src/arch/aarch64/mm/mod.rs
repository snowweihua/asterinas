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

use ostd_pod::Pod;

use crate::{
    mm::{
        dma::DmaDirection,
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags as PrivFlags},
        page_table::{PteScalar, PteTrait},
        Paddr, PagingConstsTrait, PagingLevel, PodOnce, Vaddr, PAGE_SIZE,
    },
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

/// Invalidates the local TLB and walk cache.
///
/// On Cortex-A53, `tlbi vmalle1` leaves stale intermediate walk-cache entries.
/// A TTBR0_EL1 rewrite only takes effect if the value changes (same-value
/// writes are elided), so toggle ASID bit 56 and restore: BADDR is preserved,
/// keeping transient speculative walks valid, while the changed value
/// guarantees the walk cache is flushed.
pub(crate) fn flush_tlb_and_walk_cache() {
    unsafe {
        // LOCAL `tlbi vmalle1` only. The inner-shareable variant (`vmalle1is`)
        // broadcasts to every PE in the domain; on the BCM2836 that includes
        // the VideoCore, and a broadcast the firmware does not ACK makes the
        // trailing `dsb ish` spin forever on the BSP (silent whole-system
        // stop). Each PE that modifies a page table flushes its own TLB via
        // the TTBR0 toggle below; cross-PE coherence is handled by the
        // `dispatch_tlb_flush` IPI path.
        asm!("tlbi vmalle1", options(nostack, nomem, preserves_flags));
        asm!("dsb ish", options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));

        // Kernel executes from the TTBR1 half, so transiently changing TTBR0 is safe.
        let ttbr0: u64;
        asm!("mrs {0}, ttbr0_el1", out(reg) ttbr0, options(nostack, nomem, preserves_flags));
        asm!("dsb sy", options(nostack, nomem, preserves_flags));
        asm!(
            "msr ttbr0_el1, {0}",
            in(reg) ttbr0 ^ (1u64 << 56),
            options(nostack, nomem, preserves_flags)
        );
        asm!("isb", options(nostack, nomem, preserves_flags));
        asm!("msr ttbr0_el1, {0}", in(reg) ttbr0, options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
        asm!("dsb sy", options(nostack, nomem, preserves_flags));
    }
}

pub(crate) fn tlb_flush_addr(vaddr: Vaddr) {
    let _ = vaddr;
    // Use TTBR0 rewrite on all platforms for reliable TLB + walk-cache
    // invalidation. On BCM2836/Cortex-A53, `tlbi vmalle1` invalidates TLB
    // entries but the page-table walker may retain stale intermediate-level
    // translations. Rewriting TTBR0_EL1 forces a full walk-cache flush.
    flush_tlb_and_walk_cache();
}

pub(crate) fn tlb_flush_addr_range(range: &Range<Vaddr>) {
    for vaddr in range.clone().step_by(PAGE_SIZE) {
        tlb_flush_addr(vaddr);
    }
}

pub(crate) fn tlb_flush_all_excluding_global() {
    // Use TTBR0 rewrite on all platforms, mirroring `tlb_flush_addr`: on
    // BCM2836/Cortex-A53, `tlbi vmalle1` fails to evict stale intermediate-level
    // walk-cache entries, so a full TTBR0 rewrite is required.
    flush_tlb_and_walk_cache();
}

pub(crate) fn tlb_flush_all_including_global() {
    // Use TTBR0 rewrite on all platforms, mirroring `tlb_flush_addr`.
    flush_tlb_and_walk_cache();
}

pub unsafe fn activate_page_table(root_paddr: Paddr) {
    assert!(root_paddr % PagingConsts::BASE_PAGE_SIZE == 0);
    // User roots share the kernel half, so one root serves both halves:
    // TTBR0 for user space, TTBR1 for the kernel.
    unsafe {
        asm!("dsb sy", options(nostack, nomem, preserves_flags));
        asm!("msr ttbr0_el1, {0}", in(reg) root_paddr, options(nostack, nomem, preserves_flags));
        asm!("msr ttbr1_el1, {0}", in(reg) root_paddr, options(nostack, nomem, preserves_flags));
        asm!("isb", options(nostack, nomem, preserves_flags));
    }
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

// SAFETY: A zeroed PTE is absent; `from_repr`/`to_repr` round-trip;
// `as_usize`/`from_usize` use the default derived conversions.
unsafe impl PteTrait for PageTableEntry {
    fn from_repr(repr: &PteScalar, level: PagingLevel) -> Self {
        match repr {
            PteScalar::Absent => PageTableEntry(0),
            PteScalar::PageTable(paddr, _flags) => Self::new_pt(*paddr),
            PteScalar::Mapped(paddr, prop) => Self::new_page(*paddr, level, *prop),
        }
    }

    fn to_repr(&self, level: PagingLevel) -> PteScalar {
        if self.0 & PageTableFlags::VALID.bits() == 0 {
            return PteScalar::Absent;
        }

        if self.is_last(level) {
            PteScalar::Mapped(self.paddr(), self.prop())
        } else {
            PteScalar::PageTable(
                self.paddr(),
                crate::mm::page_prop::PageTableFlags::empty(),
            )
        }
    }
}

impl PageTableEntry {
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

pub(crate) fn can_sync_dma() -> bool {
    false
}

/// Flushes the instruction cache for a user-space range after mapping code.
///
/// File data reaches RAM through the data cache, but the instruction cache
/// may hold stale lines for a recycled frame. Without this flush the CPU
/// can execute stale bytes (or fault) on the first run of freshly loaded
/// user code. Runs in the faulting process context so `ic ivau` applies.
///
/// This is safe to call from safe code: cache maintenance cannot violate
/// memory safety (at worst a fault on an unmapped address, which is
/// recoverable, or wasted work).
/// TEMP-HW-DEBUG: AT-walk the linear window base. Returns false if the walk
/// fails — i.e. the active TTBR1 root has lost its linear PGD slot (the
/// R47-50 root corruption). Revert before MR-1.
pub fn linear_window_ok() -> bool {
    let par: usize;
    unsafe {
        core::arch::asm!(
            "at s1e1r, {va}",
            "isb",
            "mrs {par}, par_el1",
            va = in(reg) crate::mm::kspace::LINEAR_MAPPING_BASE_VADDR,
            par = out(reg) par,
            options(nostack, preserves_flags),
        );
    }
    par & 1 == 0
}

pub fn flush_icache_range(start: Vaddr, len: usize) {
    let ctr: usize;
    unsafe {
        core::arch::asm!("mrs {}, ctr_el0", out(reg) ctr, options(nostack, nomem, preserves_flags));
    }
    let line_size = 4usize << ((ctr >> 16) & 0xf);

    // Cache maintenance by VA faults on addresses with no translation (the
    // bulk flush covers not-yet-committed pages). Probe each line with AT and
    // skip the ones that are unmapped; committed pages are flushed per-page
    // once they are actually faulted in.
    let mapped = |addr: usize| -> bool {
        let par: usize;
        unsafe {
            core::arch::asm!(
                "at s1e0r, {addr}",
                "isb",
                "mrs {par}, par_el1",
                addr = in(reg) addr,
                par = out(reg) par,
                options(nostack, nomem, preserves_flags),
            );
        }
        par & 1 == 0
    };

    let mut addr = start & !(line_size - 1);
    let end = start + len;
    unsafe {
        while addr < end {
            if mapped(addr) {
                core::arch::asm!("dc cvau, {0}", in(reg) addr, options(nostack, preserves_flags));
            }
            addr += line_size;
        }
        core::arch::asm!("dsb ish", options(nostack, preserves_flags));
    }
    let mut addr = start & !(line_size - 1);
    unsafe {
        while addr < end {
            if mapped(addr) {
                core::arch::asm!("ic ivau, {0}", in(reg) addr, options(nostack, preserves_flags));
            }
            addr += line_size;
        }
        core::arch::asm!("dsb ish", options(nostack, preserves_flags));
        core::arch::asm!("isb", options(nostack, preserves_flags));
    }
}

pub(crate) unsafe fn sync_dma_range<D: DmaDirection>(_range: Range<Vaddr>) {
    debug_assert!(can_sync_dma());
}

