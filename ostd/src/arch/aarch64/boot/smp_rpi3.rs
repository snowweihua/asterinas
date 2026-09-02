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

/// ARM_LOCAL peripheral base PA on RPi3.
/// Mailbox doorbell writes work at 0x3F000000 (readback 0x344000 for CPU2/CPU3).
/// Spin-table offsets are likely at a different location within ARM_LOCAL.
const ARM_LOCAL_PA: usize = 0x3F00_0000;

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

/// Check if PSCI is available by calling PSCI_VERSION.
/// TF-A exposes PSCI via SMC even if the DTB doesn't have a /psci node.
fn is_psci_available() -> bool {
    let psci_version = smc_call(0x84000000, 0, 0, 0);
    let valid = psci_version >= 0x10000 && psci_version < 0xffffffff;
    log::info!("[a2-smp] rpi3: PSCI_VERSION={:#x}, available={}", psci_version, valid);
    valid
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
    pt_ptr: Paddr,
    num_cpus: u32,
) {
    // Complete PSCI diagnostic table
    #[cfg(target_arch = "aarch64")]
    {
        // PSCI_VERSION
        let psci_version = smc_call(0x84000000, 0, 0, 0);
        log::info!("[a2-smp] rpi3: PSCI_VERSION={:#x}", psci_version);

        // PSCI_FEATURES for CPU_ON
        let psci_features_cpu_on = smc_call(0x8400000a, PSCI_CPU_ON, 0, 0);
        log::info!("[a2-smp] rpi3: PSCI_FEATURES(CPU_ON)={:#x}", psci_features_cpu_on);

        // PSCI_FEATURES for AFFINITY_INFO
        let psci_features_aff_info = smc_call(0x8400000a, PSCI_AFFINITY_INFO, 0, 0);
        log::info!("[a2-smp] rpi3: PSCI_FEATURES(AFFINITY_INFO)={:#x}", psci_features_aff_info);

        // PSCI_FEATURES for SYSTEM_RESET
        let psci_features_sys_reset = smc_call(0x8400000a, PSCI_SYSTEM_RESET, 0, 0);
        log::info!("[a2-smp] rpi3: PSCI_FEATURES(SYSTEM_RESET)={:#x}", psci_features_sys_reset);

        // PSCI_AFFINITY_INFO for ALL CPUs (including CPU0)
        for cpu_id in 0..4u32 {
            let mpidr = 0x80000000u64 | (cpu_id as u64);
            let aff_info = smc_call(PSCI_AFFINITY_INFO, mpidr, 0, 0);
            log::info!("[a2-smp] rpi3: PSCI_AFFINITY_INFO(cpu={}, mpidr={:#x})={:#x}", cpu_id, mpidr, aff_info);
        }
    }

    // Set up globals and copy boot stub BEFORE any PSCI_CPU_ON calls
    #[cfg(target_arch = "aarch64")]
    log::info!("[a2-smp] rpi3: SMP bringup starting");

    unsafe {
        __ap_boot_info_array_pointer = info_ptr;
        __boot_page_table_pointer = pt_ptr as u64;
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

    log::info!("[a2-smp] rpi3: boot stub copied to {:#x}", ap_boot_dst_pa);

    let ap_entry_paddr = ap_boot_dst_pa as u64;

    // Clear marker region before waking APs
    unsafe {
        core::ptr::write_volatile(0x41000 as *mut u8, 0x55);
    }

    // Try PSCI via SMC first
    #[cfg(target_arch = "aarch64")]
    if is_psci_available() {
        log::info!("[a2-smp] rpi3: PSCI available, trying SMC CPU_ON");
        for cpu_id in 1..num_cpus {
            let mpidr = match get_mpidr(cpu_id) {
                Some(m) => m,
                None => {
                    log::info!("[a2-smp] rpi3: no MPIDR for CPU {}", cpu_id);
                    continue;
                }
            };
            // Check AFFINITY_INFO BEFORE bringup CPU_ON to see current state
            let aff_info_before = smc_call(PSCI_AFFINITY_INFO, mpidr, 0, 0);
            log::info!("[a2-smp] rpi3: PSCI_AFFINITY_INFO before bringup CPU_ON(cpu={})={:#x}", cpu_id, aff_info_before);

            let info = &*info_ptr.add(cpu_id as usize - 1);
            let stack_top = info.stack_top as u64;
            let info_ptr_val = info as *const _ as u64;

            log::info!("[a2-smp] rpi3: PSCI CPU_ON cpu={} mpidr={:#x} entry={:#x}(PA) info={:#x}",
                cpu_id, mpidr, ap_entry_paddr, info_ptr_val);

// PSCI_CPU_ON: x0=function_id, x1=mpidr, x2=entry_pa, x3=context_id
            // The context_id (PerApRawInfo pointer) is passed to the AP in x0
            let result = smc_call(PSCI_CPU_ON, mpidr, ap_entry_paddr, info_ptr_val);
            log::info!("[a2-smp] rpi3: PSCI result={:#x}", result);

            let aff_info_immediate = smc_call(PSCI_AFFINITY_INFO, mpidr, 0, 0);
            log::info!("[a2-smp] rpi3: PSCI_AFFINITY_INFO after CPU_ON(cpu={})={:#x} (immediate)", cpu_id, aff_info_immediate);

            for _ in 0..1000 { core::hint::spin_loop(); }

            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                log::info!("[a2-smp] rpi3: AP {} started via PSCI!", cpu_id);
            }

            let aff_info_delayed = smc_call(PSCI_AFFINITY_INFO, mpidr, 0, 0);
            log::info!("[a2-smp] rpi3: PSCI_AFFINITY_INFO after CPU_ON(cpu={})={:#x} (delayed)", cpu_id, aff_info_delayed);
        }
    } else {
        log::info!("[a2-smp] rpi3: PSCI not available, using spin-table");
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
        #[cfg(target_arch = "aarch64")]
        log::info!("[a2-smp] rpi3: CPU {} spin-table@{:#x} (BCM2836 offset={:#x})", cpu_id, spin_table_addr, CPU_SPIN_TABLE_OFFSETS[cpu_idx]);

        let info_base_va = AP_INFO_BASE;

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
                val = in(reg) pt_ptr as u64,
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
        {
            use crate::arch::bcm2836_irq::CORE1_MAILBOX3_SET;
            // Use identity-mapped address for ARM_LOCAL during early boot
            let mailbox_offset = ARM_LOCAL_PA + CORE1_MAILBOX3_SET + 16 * (cpu_id as usize - 1);
            unsafe {
                core::ptr::write_volatile((mailbox_offset) as *mut u32, ap_entry_paddr as u32);
                core::arch::asm!("dsb sy", "sev", options(nostack, preserves_flags));
                let readback: u32 = core::ptr::read_volatile((mailbox_offset) as *const u32);
                log::info!("[a2-smp] rpi3: wrote mailbox@{:#x}={:#x}, readback={:#x}", mailbox_offset, ap_entry_paddr as u32, readback);
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            unsafe {
                // Use identity-mapped spin_table_addr
                core::ptr::write_volatile(spin_table_addr as *mut u64, ap_entry_paddr);
                core::arch::asm!("dsb ish", "sev", options(nostack, preserves_flags));
                let readback: u64 = core::ptr::read_volatile(spin_table_addr as *const u64);
                log::info!("[a2-smp] rpi3: spin-table@{:#x} wrote={:#x} readback={:#x}", spin_table_addr, ap_entry_paddr, readback);
            }
        }

        for _ in 0..50000 {
            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                log::info!("[a2-smp] rpi3: AP {} started via spin-table!", cpu_id);
                break;
            }
        }

        for _ in 0..500000 {
            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                log::info!("[a2-smp] rpi3: AP {} started via spin-table!", cpu_id);
                break;
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        for cpu_id in 1..num_cpus {
            let base = (cpu_id as usize) * 0x1000;
            let entry_pa = 0x42000usize + base;
            let post_putchar_pa = 0x46000usize + base;
            let after_ttbr_pa = 0x60000usize + base;
            let pre_rust_pa = 0x64000usize + base;
            let entry_val: u32 =
                unsafe { core::ptr::read_volatile(entry_pa as *const u32) };
            let post_val: u32 =
                unsafe { core::ptr::read_volatile(post_putchar_pa as *const u32) };
            let ttbr_val: u32 =
                unsafe { core::ptr::read_volatile(after_ttbr_pa as *const u32) };
            let rust_val: u32 =
                unsafe { core::ptr::read_volatile(pre_rust_pa as *const u32) };
            log::info!(
                "[a2-smp] rpi3: AP markers cpu={} entry@{:#x}={:#x} post_put@{:#x}={:#x} after_ttbr@{:#x}={:#x} pre_rust@{:#x}={:#x}",
                cpu_id,
                entry_pa, entry_val,
                post_putchar_pa, post_val,
                after_ttbr_pa, ttbr_val,
                pre_rust_pa, rust_val
            );
            let ap_early_base = 0x68000usize + base;
            let s0: u32 = unsafe { core::ptr::read_volatile(ap_early_base as *const u32) };
            let s1: u32 = unsafe { core::ptr::read_volatile((ap_early_base + 4) as *const u32) };
            let s2: u32 = unsafe { core::ptr::read_volatile((ap_early_base + 8) as *const u32) };
            let s3: u32 = unsafe { core::ptr::read_volatile((ap_early_base + 12) as *const u32) };
            let s4: u32 = unsafe { core::ptr::read_volatile((ap_early_base + 16) as *const u32) };
            let s5: u32 = unsafe { core::ptr::read_volatile((ap_early_base + 20) as *const u32) };
            let s6: u32 = unsafe { core::ptr::read_volatile((ap_early_base + 24) as *const u32) };
            log::info!(
                "[a2-smp] rpi3: AP ap_early_entry cpu={} s0={:#x} s1={:#x} s2={:#x} s3={:#x} s4={:#x} s5={:#x} s6={:#x}",
                cpu_id, s0, s1, s2, s3, s4, s5, s6
            );
        }
    }

    #[cfg(target_arch = "aarch64")]
    log::info!("[a2-smp] rpi3: SMP bringup done");

// Check if AP boot marker was written (at 0x41000 from ap_boot.S)
    #[cfg(target_arch = "aarch64")]
    {
        let ap_marker = unsafe { core::ptr::read_volatile(0x41000 as *const u64) };
        log::info!("[a2-smp] rpi3: AP boot marker @0x41000={:#x} (should be 0xABCD if AP reached boot code)", ap_marker);

        for cpu_id in 1..num_cpus {
            let marker_pa = 0x42000usize + (cpu_id as usize) * 0x1000;
            let marker: u32 =
                unsafe { core::ptr::read_volatile(marker_pa as *const u32) };
            if marker == 0x50 + cpu_id {
                log::info!("[a2-smp] rpi3: AP {} stub entry marker found, reporting online", cpu_id);
                crate::boot::smp::report_online_and_hw_cpu_id(cpu_id);
            }
        }
    }
}
