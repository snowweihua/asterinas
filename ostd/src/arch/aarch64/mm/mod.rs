// SPDX-License-Identifier: MPL-2.0
use alloc::fmt;
use core::ops::Range;
use core::arch::asm;
use aarch64_cpu::registers::*;
//TODO:: some code could be optimized from crate::aarch64-paging

use crate::{
    mm::{
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags as PrivFlags},
        page_table::PageTableEntryTrait,
        Paddr, PagingConstsTrait, PagingLevel, PodOnce, Vaddr, PAGE_SIZE,
    },
    Pod,
};

pub(crate) const NR_ENTRIES_PER_PAGE: usize = 512;

#[derive(Clone, Debug, Default)]
pub struct PagingConsts {}

impl PagingConstsTrait for PagingConsts {
    const BASE_PAGE_SIZE: usize = 4096;
    const NR_LEVELS: PagingLevel = 4;
    const ADDRESS_WIDTH: usize = 48;
    const VA_SIGN_EXT: bool = true;
    const HIGHEST_TRANSLATION_LEVEL: PagingLevel = 4;
    const PTE_SIZE: usize = size_of::<PageTableEntry>();
}

bitflags::bitflags! {
    #[derive(Pod)]
    #[repr(C)]
    /// Possible flags for a page table entry.
    pub struct PageTableFlags: usize {
        /// Specifies whether the mapped frame or page table is valid.
        const VALID =           1 << 0;
        /// TYPE block:0, table:1
        const TYPE =            1 << 1;
        /// Access permissions.
        const EL1_EL0 =         1 << 6;
        const WRITABLE =        1 << 7;
        /// Shareability 
        const SHAREABLE =       1 << 8;
        /// Whether the memory area represented by this entry is accessed.
        const ACCESSED =        1 << 10;
        /// Indicates that the page has been modified.
        const DIRTY  =        1 << 51;    
        /// Privileged execute-never.
        const PXN =              1 << 53;    
        /// Unprivileged execute-never.
        const UXN =              1 << 54;
        }
}

// Define the memory attribute fields for MAIR_EL1.
// These bit patterns are defined in the ARM Architecture Reference Manual (ARM ARM).
// Attr field (bits 7:4): Outer Cacheability, (bits 3:0): Inner Cacheability.
const MAIR_ATTR_NOCACHEABLE: u64 = 0b0100_0100; // Inner/Outer no-cacheable
const MAIR_ATTR_WRITE_BACK: u64 = 0b1111_1111;   // Inner/Outer Write-Back
const MAIR_ATTR_DEVICE_NGNRE: u64 = 0b0000_0000; // Device_nGnRnE (uncacheable)

// The complete MAIR value combines these attributes at different index positions.
// Index 0: Device
// Index 1: Write-Back
// Index 2: Nocacheable
const MAIR_VALUE: u64 = (MAIR_ATTR_DEVICE_NGNRE << 0) |
                         (MAIR_ATTR_WRITE_BACK << 8) |
                         (MAIR_ATTR_NOCACHEABLE << 16);

// During boot, you would set the register with this value.
// unsafe { MAIR_EL1.set(MAIR_VALUE); }


// Constants for bit masks in a level 3 page descriptor.
const ATTRINDX_MASK: u64 = 0b111 << 2;
const SH_MASK: u64 = 0b11 << 8;

// Assume the following are configured in your MAIR_EL1 during boot.
// These constants specify the AttrIndx corresponding to each policy.
const ATTRINDX_DEVICE: u64 = 0b000 << 2; // Index 0
const ATTRINDX_WRITE_BACK: u64 = 0b001 << 2; // Index 1
const ATTRINDX_UNCACHEABLE: u64 = 0b010 << 2; // Index 2

// Shareability values (from the ARM ARM):
const SH_NON_SHAREABLE: u64 = 0b00 << 8;
const SH_OUTER_SHAREABLE: u64 = 0b10 << 8;
const SH_INNER_SHAREABLE: u64 = 0b11 << 8;

pub(crate) fn tlb_flush_addr(vaddr: Vaddr) {
    unsafe {
    // 1. Ensure all previous memory writes (e.g., page table updates) are visible.
    //    The "ishst" (Inner Shareable System Halted) variant ensures all writes are observed
    //    before the TLBI instruction.
    asm!("dsb ishst", options(nostack, nomem, preserves_flags));

        // Invalidate the TLB entry.
        // Use 'in(reg)' or 'in(xreg)' for a generic 64-bit register constraint.
        // The compiler will pick a register and insert its name into the {0} placeholder.
    asm!("tlbi vae1is, {0}", in(reg) vaddr.as_usize() as u64, options(nostack, nomem, preserves_flags));

    // 3. Ensure the TLB invalidation completes and subsequent instructions see it.
    //    "dsb ish" (Inner Shareable) ensures the TLB invalidation completes.
    //    "isb" (Instruction Synchronization Barrier) ensures the CPU refetches instructions
    //    after the TLB state has been updated.
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
    // 1. Ensure all previous memory writes (e.g., page table updates) are visible.
    //    DSB ISHST ensures all writes are observed before the TLBI instruction on all Inner Shareable PEs.
    asm!("dsb ishst", options(nostack, nomem, preserves_flags));

    // 2. Invalidate all non-global TLB entries for the current ASID.
    //    TLBI ASIDE1IS invalidates EL1&0 TLB entries associated with the current ASID and marked as non-global (nG).
    //    The register 'x0' will implicitly contain the current ASID and is ignored for this specific TLBI variant.
    asm!("tlbi aside1is", options(nostack, nomem, preserves_flags));

    // 3. Ensure the TLB invalidation completes and subsequent instructions see the updated state.
    //    DSB ISH ensures the TLB invalidation completes across the Inner Shareable domain.
    //    ISB flushes the pipeline, ensuring subsequent instructions are fetched with the new TLB state.
    asm!("dsb ish", options(nostack, nomem, preserves_flags));
    asm!("isb", options(nostack, nomem, preserves_flags));
}

pub(crate) fn tlb_flush_all_including_global() {
    // 1. Ensure all previous memory writes (e.g., page table updates) are visible.
    //    DSB ISHST ensures all writes are observed before the TLBI instruction on all Inner Shareable PEs.
    asm!("dsb ishst", options(nostack, nomem, preserves_flags));

    // 2. Invalidate all TLB entries for EL1, including global entries, across the Inner Shareable domain.
    //    TLBI ALLE1IS performs this operation.
    //    The register 'x0' (or any other general-purpose register) is used as a dummy operand,
    //    as TLBI ALLE1IS does not take an explicit address or ASID, but the `asm!` macro
    //    requires an input or output for some TLBI instructions.
    asm!("tlbi alle1is", options(nostack, nomem, preserves_flags));

    // 3. Ensure the TLB invalidation completes and subsequent instructions see the updated state.
    //    DSB ISH ensures the TLB invalidation completes across the Inner Shareable domain.
    //    ISB flushes the pipeline, ensuring subsequent instructions are fetched with the new TLB state.
    asm!("dsb ish", options(nostack, nomem, preserves_flags));
    asm!("isb", options(nostack, nomem, preserves_flags));
}

#[derive(Clone, Copy, Pod, Default)]
#[repr(C)]
pub struct PageTableEntry(usize);

/// Activates a new page table by setting TTBR0_EL1 on AArch64.
///
/// This function is unsafe because:
/// 1. It directly manipulates system registers, which can lead to system instability if done incorrectly.
/// 2. It changes the memory mapping, which can cause subsequent memory accesses to fail if the new
///    page table is not correctly configured or the calling code is not prepared for the change.
///
/// `root_paddr`: The physical address of the new root page table (level 0 or 1).
/// `_root_pt_cache`: Placeholder for future cache policy application (currently ignored).
pub unsafe fn activate_page_table(root_paddr: Paddr, _root_pt_cache: CachePolicy) {
    assert!(root_paddr % PagingConsts::BASE_PAGE_SIZE == 0);
    let ppn = root_paddr >> 12;
    unsafe {
    // 1. Ensure all preceding memory writes (e.g., page table construction) are visible.
    //    DSB ISHST ensures all writes are observed before the TTBR update.
    asm!("dsb ishst", options(nostack, nomem, preserves_flags));

    // 2. Set the Translation Table Base Register 0 (TTBR0_EL1) with the physical address
    //    of the new root page table. TTBR0_EL1 typically handles the lower half of the
    //    virtual address space (often user space).
    //    The register 'x0' will contain the physical address `root_paddr`.
    asm!("msr ttbr0_el1, {0}", in(reg) root_paddr, options(nostack, nomem, preserves_flags));

    // 3. Invalidate all existing TLB entries, as the entire page table mapping has changed.
    //    TLBI ALLE1IS invalidates all EL1 TLB entries (including global) across the Inner Shareable domain.
    asm!("tlbi alle1is", options(nostack, nomem, preserves_flags));

    // 4. Ensure the TLB invalidation completes and subsequent instructions see the updated state.
    //    DSB ISH ensures the TLB invalidation completes.
    //    ISB flushes the pipeline, ensuring subsequent instructions are fetched with the new TLB state.
    asm!("dsb ish", options(nostack, nomem, preserves_flags));
    asm!("isb", options(nostack, nomem, preserves_flags));
    }
// Note: If you also need to set TTBR1_EL1 (for the kernel's upper virtual address space),
// you would perform a similar `msr ttbr1_el1, {0}` operation.
// The `_root_pt_cache` parameter is currently ignored but can be used in the future
// to set MAIR_EL1 or other memory attribute registers to control caching for the page table itself.
}

pub fn current_page_table_paddr() -> Paddr {
    let mut root_paddr: Paddr;

    // boot_l4pt in boot.S
    // Read the value from TTBR0_EL1 into a general-purpose register (x0 in this case).
    // The "mrs" (Move Register to System Register) instruction is used for this purpose.
    asm!("mrs {0}, ttbr0_el1", out(reg) root_paddr, options(nostack, nomem, preserves_flags));

    root_paddr
}

impl PageTableEntry {
    const PHYS_ADDR_MASK: usize = 0x0000_FFFF_FFFF_FFFF;


}

/// Parse a bit-flag bits `val` in the representation of `from` to `to` in bits.
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
        Self(paddr & Self::PHYS_ADDR_MASK | PageTableFlags::VALID.bits())
    }

    fn paddr(&self) -> Paddr {
        self.0 & Self::PHYS_ADDR_MASK
    }

        // Implementation of fn cache_policy

    fn prop(&self) -> PageProperty {
        let flags = (parse_flags!(self.0, PageTableFlags::PXN, PageFlags::X))
            | (parse_flags!(self.0, PageTableFlags::ACCESSED, PageFlags::ACCESSED))
            | (parse_flags!(self.0, PageTableFlags::DIRTY, PageFlags::DIRTY))
            | (parse_flags!(self.0, PageTableFlags::RSV2, PageFlags::AVAIL2));
        let priv_flags = (parse_flags!(self.0, PageTableFlags::EL1_EL0, PrivFlags::USER))
            | (parse_flags!(self.0, PageTableFlags::RSV1, PrivFlags::AVAIL1));
        
        if self.0 & PageTableFlags::WRITABLE.bits() != 0 {
            flags |= PageFlags::W.bits() as usize;
            flags |= PageFlags::R.bits() as usize;
        } else {
            flags |= PageFlags::R.bits() as usize;
            flags &= !(PageFlags::W.bits() as usize);
        } 

        // Step 1: Extract the AttrIndx from the PTE (bits 2-4 for a page descriptor).
        let attr_indx = ((self.0 & ATTRINDX_MASK) >> 2) as usize;

        // Step 2: Read the MAIR_EL1 register. This requires aarch64-cpu and is unsafe.
        // It assumes the MAIR is configured early in the boot process and not changed later.
        let mair_value = unsafe { MAIR_EL1.get() };

        // Step 3: Extract the relevant memory attribute field from MAIR_EL1.
        // Each entry in MAIR is 8 bits, so we shift by attr_indx * 8.
        let mair_entry = (mair_value >> (attr_indx * 8)) & 0xFF;

        // Step 4: Interpret the memory attribute to determine the CachePolicy.
        // This is a simplified interpretation. A real kernel would need a more
        // robust mapping based on the full memory attribute bits.
        let cache = match mair_entry {
            // Check for the known configurations.
            0b0000_0000 => CachePolicy::Device, // Device_nGnRnE
            0b0100_0100 => CachePolicy::Uncacheable,
            0b1111_1111 => CachePolicy::WriteBack,
            _ => {
                // If it doesn't match, determine a reasonable default.
                // A common fallback for unknown Normal Memory attributes is WriteBack.
                CachePolicy::WriteBack
            }
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
            | parse_flags!(prop.flags.bits(), PageFlags::X, PageTableFlags::PXN)
            | parse_flags!(prop.priv_flags.bits(), PrivFlags::USER, PageTableFlags::EL1_EL0)
            | parse_flags!(prop.priv_flags.bits(), PrivFlags::AVAIL1, PageTableFlags::RSV1)
            | parse_flags!(prop.flags.bits(), PageFlags::AVAIL2, PageTableFlags::RSV2);

        if prop.flags.bits() & PageFlags::W.bits() as usize != 0 {
            flags |= PageTableFlags::WRITABLE.bits() | PageTableFlags::READABLE.bits();
        } else {
            flags |= PageTableFlags::READABLE.bits();
        }
        // First, clear the old attributes to ensure we start from a clean state.
        self.0 &= !(ATTRINDX_MASK | SH_MASK);

        match prop.cache {
            CachePolicy::WriteBack => {
                self.0 |= ATTRINDX_WRITE_BACK | SH_INNER_SHAREABLE;
            }
            CachePolicy::Uncacheable => {
                self.0 |= ATTRINDX_UNCACHEABLE | SH_OUTER_SHAREABLE;
            }
            CachePolicy::Device => {
                self.0 |= ATTRINDX_DEVICE | SH_OUTER_SHAREABLE;
            }
            CachePolicy::WriteThrough => {
                self.0 |= ATTRINDX_UNCACHEABLE | SH_INNER_SHAREABLE;
            }
            _ => panic!("unsupported cache policy"),
        }        

        self.0 = (self.0 & Self::PHYS_ADDR_MASK) | flags;
    }

    fn is_last(&self, level: PagingLevel) -> bool {
        let rwx = PageTableFlags::READABLE | PageTableFlags::WRITABLE | PageTableFlags::EXECUTABLE;
        level == 1 || (self.0 & rwx.bits()) != 0
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
    // TODO: Implement this fallible operation.

}

pub(crate) unsafe fn __memset_fallible(dst: *mut u8, value: u8, size: usize) -> usize {
    // TODO: Implement this fallible operation.

}

pub(crate) unsafe fn __atomic_load_fallible(ptr: *const u32) -> u64 {
    // TODO: Implement this fallible operation.

}

pub(crate) unsafe fn __atomic_cmpxchg_fallible(ptr: *mut u32, old_val: u32, new_val: u32) -> u64 {
    // TODO: Implement this fallible operation.

}
