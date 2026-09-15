// SPDX-License-Identifier: MPL-2.0

//! RPi3 SMP bringup via BCM2836 spin-table with runtime AP code copy.
//!
//! ## Memory Layout (RPi3, PA == VA for low 4GB with identity mapping)
//! - Boot stub copied to: PA 0x40000 (4KB region)
//! - AP info region:      PA 0x50000 (3 x 8-byte values = 24 bytes)
//! - AP marker region:    PA 0x41000 (8 bytes, offsets 0-7)
//!
//! The AP info region is SEPARATE from the boot stub copy area to avoid
//! any overlap with BSP self-test writes that may touch the same cache lines.
//!
//! ## AP Info Region Layout at PA 0x50000
//! - Offset 0x00: __aps_hold_flag (u64) - set to 1 when entry is ready
//! - Offset 0x08: __aps_entry (u64) - entry PA for AP to branch to
//! - Offset 0x10: __boot_pt_root (u64) - page table root PA
//! - Offset 0x18: __info_array (u64) - PerApRawInfo array pointer
//!
//! ## Protocol (Two-Phase)
//! 1. BSP copies boot stub to PA 0x40000, cleans caches
//! 2. BSP writes hold_flag=1, entry=0x40000, pt_root, info_array to 0x50000
//! 3. BSP triggers BCM2836 mailbox IRQ to wake AP from WFE
//! 4. AP wakes, enables MMU, polls hold_flag at 0x50000
//! 5. AP reads entry from 0x50000 + 0x08 and branches to it
//!
//! # Safety
//!
//! The stub/info/marker regions above are reserved by layout and accessed
//! as raw physical memory only while the boot identity map is live. The
//! BSP completes all info writes plus cache cleaning before the mailbox
//! wakeup; APs poll `hold_flag` before branching to the entry.

use super::DEVICE_TREE;
use crate::{
    boot::smp::PerApRawInfo,
    mm::Paddr,
};

unsafe extern "C" {
    static __ap_boot_start: u8;
    static __ap_boot_end: u8;
    static mut __ap_boot_info_array_pointer: *const PerApRawInfo;
    static mut __boot_page_table_pointer: u64;
}

/// AP info region base PA - SEPARATE from boot stub copy area.
/// This is in a different 4KB page (0x50000) than the boot stub.
/// This avoids any cache line sharing issues with BSP self-test writes.
const AP_INFO_BASE: usize = 0x5_0000;

/// ARM_LOCAL peripheral base PA on RPi3 (QA7 ARM-local block).
/// Spin-table cpu-release-addr registers and mailbox doorbells live here.
const ARM_LOCAL_PA: usize = 0x4000_0000;

/// BCM2836 spin-table offsets per CPU (within ARM_LOCAL peripheral space).
/// These are the actual spin-table addresses within ARM_LOCAL - NOT the mailbox addresses.
/// Match the values in bcm2836_irq.rs: CORE1_BOOT_CONTROL=0xE8, CORE2_BOOT_CONTROL=0xF0, etc.
const CPU_SPIN_TABLE_OFFSETS: [usize; 4] = [
    0xE0, // CPU 0
    0xE8, // CPU 1
    0xF0, // CPU 2
    0xF8, // CPU 3
];

/// PSCI function IDs for RPi3 (using SMC conduit)
/// These are PSCI v0.2 standard function IDs (32-bit SMCCC calling convention)
/// Reference: Linux kernel include/uapi/linux/psci.h
const PSCI_CPU_OFF: u64 = 0x84000002;          // CPU_OFF function ID
const PSCI_CPU_ON: u64 = 0x84000003;           // CPU_ON function ID (was incorrectly 0x84000001)
const PSCI_AFFINITY_INFO: u64 = 0x84000004;    // AFFINITY_INFO function ID (was incorrectly 0x84000001)
const PSCI_SYSTEM_RESET: u64 = 0x84000009;     // SYSTEM_RESET function ID
const PSCI_SUCCESS: u64 = 0;

/// Get the MPIDR_EL1 for a given CPU index.
/// For RPi3 BCM2837, the MPIDR is 0x80000000 | cpu_index.
fn get_mpidr(cpu_index: u32) -> Option<u64> {
    if cpu_index < 4 {
        Some(0x80000000u64 | (cpu_index as u64))
    } else {
        None
    }
}

/// Reads CurrentEL (returns 1, 2, or 3). Plain MRS, cannot fault.
fn current_el() -> u64 {
    let el: u64;
    unsafe {
        core::arch::asm!(
            "mrs {0}, CurrentEL",
            out(reg) el,
            options(nomem, nostack, preserves_flags),
        );
    }
    (el >> 2) & 0x3
}

/// Returns true if EL3 is implemented (ID_AA64PFR0_EL1[15:12] != 0).
/// SMC without EL3 is architecturally UNDEFINED, so a missing EL3 means
/// no PSCI firmware can exist and any smc call would fault or hang
/// (observed under QEMU raspi3b, which provides no EL3 firmware).
/// Plain MRS, cannot fault.
fn el3_present() -> bool {
    let pfr0: u64;
    unsafe {
        core::arch::asm!(
            "mrs {0}, ID_AA64PFR0_EL1",
            out(reg) pfr0,
            options(nomem, nostack, preserves_flags),
        );
    }
    ((pfr0 >> 12) & 0xf) != 0
}

/// Reads the generic-timer frequency (Hz). Plain MRS, cannot fault.
fn cntfrq() -> u64 {
    let freq: u64;
    unsafe {
        core::arch::asm!(
            "mrs {0}, cntfrq_el0",
            out(reg) freq,
            options(nomem, nostack, preserves_flags),
        );
    }
    freq
}

/// True when PSCI firmware can handle SMC: EL3 must exist, and the timer
/// frequency must match the Pi's 19.2MHz crystal programmed by VideoCore
/// firmware. QEMU leaves its own default frequency, so this distinguishes
/// real hardware (TF-A present) from emulation (no firmware) without
/// executing a potentially hanging smc.
pub(crate) fn psci_usable() -> bool {
    el3_present() && cntfrq() == 19_200_000
}

/// Check if PSCI is available by calling PSCI_VERSION.
/// TF-A exposes PSCI via SMC even if the DTB doesn't have a /psci node.
/// Skipped entirely when EL3 is absent, where smc cannot be handled.
fn is_psci_available() -> bool {
    if !psci_usable() {
        return false;
    }
    let psci_version = smc_call(0x84000000, 0, 0, 0);
    // Accept real PSCI versions (0.1/0.2 as 1..2, 1.x as 0x10000+) while
    // rejecting the echoed function ID (0x84000000) and NOT_SUPPORTED (-1)
    // that a missing firmware stub returns.
    psci_version >= 1 && psci_version < 0x01000000
}

/// Read the `cpu-release-addr` property from the DTB for the given logical CPU index.
fn get_cpu_release_addr(cpu_index: u32) -> Option<u64> {
    let fdt = match DEVICE_TREE.get() {
        Some(f) => f,
        None => {
            log::info!("[a2-smp] rpi3: DEVICE_TREE.get()=None");
            return None;
        }
    };
    let cpus = match fdt.find_node("/cpus") {
        Some(c) => c,
        None => {
            log::info!("[a2-smp] rpi3: /cpus node not found");
            return None;
        }
    };

    let mut current_cpu: u32 = 0;
    for child in cpus.children() {
        if child.reg().is_some() {
            if current_cpu == cpu_index {
                if let Some(prop) = child.property("cpu-release-addr") {
                    let v = prop.value;
                    let addr = if v.len() >= 8 {
                        u64::from_be_bytes(v[0..8].try_into().ok()?)
                    } else if v.len() >= 4 {
                        u32::from_be_bytes(v[0..4].try_into().ok()?) as u64
                    } else {
                        log::info!("[a2-smp] rpi3: cpu-release-addr too short");
                        return None;
                    };
                    log::info!("[a2-smp] DTB cpu{} release-addr={:#x} (raw from DTB)", cpu_index, addr);
                    // DTB cpu-release-addr is typically the absolute PA of the spin-table entry.
                    // For RPi3, the VideoCore firmware sets this to offsets (0xe0, 0xe8, 0xf0).
                    // U-Boot's spin_table_update_dt() may have modified these to its own addresses.
                    return Some(addr);
                }
                log::info!("[a2-smp] rpi3: no cpu-release-addr prop");
                return None;
            }
            current_cpu += 1;
        }
    }
    log::info!("[a2-smp] rpi3: cpu not found in DT");
    None
}

fn smc_call(func: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "smc #0",
            inout("x0") func => result,
            in("x1") arg0,
            in("x2") arg1,
            in("x3") arg2,
        );
    }
    result
}

unsafe extern "C" {
    fn ap_boot_entry();
}

pub(crate) unsafe fn bringup_all_aps_rpi3(
    info_ptr: *const PerApRawInfo,
    _pt_ptr: Paddr,
    num_cpus: u32,
) {
    // Set up globals and copy boot stub BEFORE any PSCI_CPU_ON calls.
    // The AP info array is published to identity-mapped scratch so the
    // MMU-off stub and the PSCI context ID use physical addresses.
    // APs receive the boot-table root: only it offers identity, kernel and
    // linear mappings from one root. The active root is the kernel table.
    let boot_root = super::smp::boot_root_paddr();
    unsafe {
        super::smp::publish_ap_info_to_scratch(info_ptr, num_cpus);
        if let Some(root) = crate::mm::kspace::kernel_page_table_root_paddr() {
            super::smp::publish_kpt_root_to_scratch(root);
        }
        __ap_boot_info_array_pointer = super::smp::AP_INFO_SCRATCH_PA as *const PerApRawInfo;
        __boot_page_table_pointer = boot_root as u64;
        super::smp::flush_ap_boot_globals();
        // TEMP-HW-DEBUG: coerce the KPT singleton (read by APs during their
        // page-table switch) to PoC before release. See flush_kpt_for_ap.
        crate::mm::kspace::flush_kpt_for_ap();
    }

    let ap_boot_size = unsafe {
        (&__ap_boot_end as *const u8 as usize) - (&__ap_boot_start as *const u8 as usize)
    };
    let ap_boot_src = unsafe { &__ap_boot_start as *const u8 };

    // Derive the stub destination PA from the linker symbol so it stays at the
    // stub's own linked runtime location regardless of build/layout shifts.
    // A fixed destination (e.g. 0x344000) can overlap live kernel .text and
    // rewrite running code, which deterministically kills AP wake-up.
    let ap_boot_dst_pa = crate::arch::board::dram_base()
        + (ap_boot_src as usize - crate::mm::kspace::kernel_loaded_offset());
    let ap_boot_dst_va = crate::mm::paddr_to_vaddr(ap_boot_dst_pa);

    unsafe {
        core::ptr::copy_nonoverlapping(ap_boot_src, ap_boot_dst_va as *mut u8, ap_boot_size);
        for offset in (0..ap_boot_size).step_by(64) {
            core::arch::asm!(
                "dc cvac, {addr}",
                addr = in(reg) ap_boot_dst_va + offset,
                options(nostack, preserves_flags)
            );
        }
        core::arch::asm!("dsb ish", options(nostack, preserves_flags));
    }

    let ap_entry_paddr = ap_boot_dst_pa as u64;

    // Clear marker region before waking APs (linear alias: BSP runs with
    // the MMU on and its TTBR0 has no low-half mappings).
    unsafe {
        let marker_va = crate::mm::kspace::paddr_to_vaddr(0x41000);
        core::ptr::write_volatile(marker_va as *mut u8, 0x55);
    }

    // TEMP-HW-DEBUG: BSP-side view of the KPT singleton word for comparison
    // against the AP-side view (loss-tolerant nibble markers). Revert.
    crate::arch::serial::marker(b'K');
    crate::arch::serial::marker_hex(crate::mm::kspace::debug_read_kpt_word());

    // Try PSCI via SMC first
    #[cfg(target_arch = "aarch64")]
    // TEMP-HW-DEBUG: when PSCI is available, use it exclusively. The extra
    // mailbox/spin-table writes below can release TF-A-held secondaries
    // WITHOUT PSCI context (garbage x0), which then fault on garbage
    // stack/TPIDR. Revert before MR-1.
    #[cfg(target_arch = "aarch64")]
    if is_psci_available() {
        for cpu_id in 1..num_cpus {
            let mpidr = match get_mpidr(cpu_id) {
                Some(m) => m,
                None => {
                    log::info!("[a2-smp] rpi3: no MPIDR for CPU {}", cpu_id);
                    continue;
                }
            };
            let info_pa =
                super::smp::AP_INFO_SCRATCH_PA as u64 + (cpu_id as u64 - 1) * 16;

// PSCI_CPU_ON: x0=function_id, x1=mpidr, x2=entry_pa, x3=context_id
            // The context_id (scratch PA of the AP's PerApRawInfo entry) is
            // passed to the AP in x0
            let _result = smc_call(PSCI_CPU_ON, mpidr, ap_entry_paddr, info_pa);

            for _ in 0..1000 { core::hint::spin_loop(); }
        }
    }

    // Skip Trusted Mailbox - it requires specific TF-A configuration
    // that may not be present. Focus on spin-table method.

    // Fall back to spin-table if PSCI didn't work
    for cpu_id in 1..num_cpus {
        // Use BCM2836 spin-table offsets directly, not from DTB
        // DTB values may have been modified by U-Boot's spin_table_update_dt()
        let cpu_idx = cpu_id as usize;
        if cpu_idx >= CPU_SPIN_TABLE_OFFSETS.len() {
            log::info!("[a2-smp] rpi3: CPU {} out of range", cpu_id);
            continue;
        }
        let spin_table_addr = ARM_LOCAL_PA + CPU_SPIN_TABLE_OFFSETS[cpu_idx];

        let info_base_va = crate::mm::kspace::paddr_to_vaddr(AP_INFO_BASE);

        unsafe {
            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}, #8]",
                "dc cvac, {addr}",
                addr = in(reg) info_base_va,
                val = in(reg) ap_entry_paddr,
                options(nostack, preserves_flags)
            );

            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}]",
                "dc cvac, {addr}",
                addr = in(reg) info_base_va,
                val = in(reg) 1u64,
                options(nostack, preserves_flags)
            );

            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}, #16]",
                "dc cvac, {addr}",
                addr = in(reg) info_base_va,
                val = in(reg) boot_root as u64,
                options(nostack, preserves_flags)
            );

            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}, #24]",
                "dc cvac, {addr}",
                "dsb ish",
                addr = in(reg) info_base_va,
                val = in(reg) info_ptr as u64,
                options(nostack, preserves_flags)
            );
        }

        #[cfg(target_arch = "aarch64")]
        if false && is_psci_available() {
            use crate::arch::bcm2836_irq::CORE1_MAILBOX3_SET;
            // Use the linear alias: the BSP runs with the MMU on (kernel
            // page table, no low-half mappings), unlike early boot code.
            let mailbox_offset = ARM_LOCAL_PA + CORE1_MAILBOX3_SET + 16 * (cpu_id as usize - 1);
            let mailbox_va =
                crate::mm::kspace::paddr_to_vaddr(mailbox_offset);
            unsafe {
                core::ptr::write_volatile(mailbox_va as *mut u32, ap_entry_paddr as u32);
                core::arch::asm!(
                    "dc cvac, {addr}",
                    addr = in(reg) mailbox_va,
                    options(nostack, preserves_flags),
                );
                core::arch::asm!("dsb sy", "sev", options(nostack, preserves_flags));
            }
        }

        #[cfg(target_arch = "aarch64")]
        if false && is_psci_available() {
            unsafe {
                // Linear alias (see above); the AP reads it with the MMU off.
                let spin_va = crate::mm::kspace::paddr_to_vaddr(spin_table_addr);
                core::ptr::write_volatile(spin_va as *mut u64, ap_entry_paddr);
                core::arch::asm!(
                    "dc cvac, {addr}",
                    addr = in(reg) spin_va,
                    options(nostack, preserves_flags),
                );
                core::arch::asm!("dsb ish", "sev", options(nostack, preserves_flags));
            }
        }

        // QEMU wake: its AArch64 secondary stub polls 64-bit slots at absolute
        // 0xD8+mpidr*8 (CPU1 -> 0xE0); PSCI is unavailable there. Push the
        // write to PoC (secondaries read with MMU off) and signal with sev.
        #[cfg(target_arch = "aarch64")]
        if !is_psci_available() {
            let slot_pa = 0xD8usize + (cpu_id as usize) * 8;
            let slot_va = crate::mm::kspace::paddr_to_vaddr(slot_pa);
            unsafe {
                core::ptr::write_volatile(slot_va as *mut u64, ap_entry_paddr);
                core::arch::asm!(
                    "dc cvac, {addr}",
                    addr = in(reg) slot_va,
                    options(nostack, preserves_flags),
                );
                core::arch::asm!("dsb sy", "sev", options(nostack, preserves_flags));
            }
        }

    }
}
