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

/// AP boot stub destination PA in raw binary.
/// .ap_boot is at file offset 0x2c4000 in the raw binary.
/// Kernel loads at PA 0x80000, so actual PA = 0x80000 + 0x2c4000 = 0x344000.
const AP_BOOT_DEST_PA: usize = 0x3_44000;
/// AP info region base PA - SEPARATE from boot stub copy area.
/// This is in a different 4KB page (0x50000) than the boot stub (0x34000).
/// This avoids any cache line sharing issues with BSP self-test writes.
const AP_INFO_BASE: usize = 0x5_0000;

/// ARM_LOCAL peripheral base PA on RPi3.
/// The BCM2836 spin-table addresses are offsets within this peripheral.
const ARM_LOCAL_PA: usize = 0x4000_0000;

/// BCM2836 spin-table offsets per CPU (within ARM_LOCAL peripheral space).
/// These are the addresses where the DTB's cpu-release-addr values should point.
const CPU_SPIN_TABLE_OFFSETS: [usize; 4] = [
    0x0D8, // CPU 0
    0x0E0, // CPU 1
    0x0E8, // CPU 2
    0x0F0, // CPU 3
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

        // PSCI_CPU_ON for each secondary CPU
        log::info!("[a2-smp] rpi3: --- PSCI_CPU_ON tests ---");
        for cpu_id in 1..4u32 {
            let mpidr = 0x80000000u64 | (cpu_id as u64);
            let result = smc_call(PSCI_CPU_ON, mpidr, AP_BOOT_DEST_PA as u64, 0u64);
            log::info!("[a2-smp] rpi3: PSCI_CPU_ON(cpu={}, mpidr={:#x}, entry={:#x})={:#x}",
                cpu_id, mpidr, AP_BOOT_DEST_PA as u64, result);
        }
    }

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
    let ap_boot_dst_va = crate::mm::paddr_to_vaddr(AP_BOOT_DEST_PA);

    // Copy boot stub to PA 0x40000
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

    // BSP self-test: verify marker region at PA 0x41000 is writable
    #[cfg(target_arch = "aarch64")]
    {
        let marker_test: u8 = 0xBC;
        unsafe {
            core::ptr::write_volatile(0x41000 as *mut u8, marker_test);
            core::arch::asm!("dsb ish");
            let readback = core::ptr::read_volatile(0x41000 as *const u8);
            log::info!("[a2-smp] marker test PA 0x41000: wrote={:#x}, read={:#x}", marker_test, readback);
            core::ptr::write_volatile(0x41000 as *mut u8, 0x55);
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        let test_val: u64 = 0xDEADBEEF;
        unsafe {
            core::ptr::write_volatile(0xe0 as *mut u64, test_val);
            core::arch::asm!("dsb ish");
            let readback: u64 = core::ptr::read_volatile(0xe0 as *const u64);
            log::info!("[a2-smp] marker test PA 0xe0 (ARM_LOCAL spin-table): wrote={:#x}, read={:#x}", test_val, readback);
            core::ptr::write_volatile(0xe0 as *mut u64, 0u64);
        }
    }

    // Test TF-A Trusted Mailbox at 0x10000008 (Secure SRAM)
    // This is where TF-A expects CPU_ON to write the GO state
    // Also re-read DTB cpu-release-addr to see if U-Boot modified it
    #[cfg(target_arch = "aarch64")]
    {
        let tm_base: u64 = 0x10000008; // Trusted Mailbox hold base for CPU0
        for cpu_id in 0..4u32 {
            let test_val: u64 = 0xDEADCAFEBABE0000u64 | (cpu_id as u64);
            let addr = tm_base + (cpu_id as u64) * 8;
            unsafe {
                core::ptr::write_volatile(addr as *mut u64, test_val);
                core::arch::asm!("dsb sy");
                let readback: u64 = core::ptr::read_volatile(addr as *const u64);
                log::info!("[a2-smp] trusted_mailbox CPU{} @ {:#x}: wrote={:#x}, read={:#x}",
                    cpu_id, addr, test_val, readback);
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    log::info!("[a2-smp] rpi3: boot stub copied");

    let ap_entry_paddr = AP_BOOT_DEST_PA as u64;

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
            let info = &*info_ptr.add(cpu_id as usize - 1);
            let stack_top = info.stack_top as u64;

            log::info!("[a2-smp] rpi3: PSCI CPU_ON cpu={} mpidr={:#x} entry={:#x}(PA) stack={:#x}",
                cpu_id, mpidr, ap_entry_paddr, stack_top);

            let result = smc_call(PSCI_CPU_ON, mpidr, ap_entry_paddr, 0u64);
            log::info!("[a2-smp] rpi3: PSCI result={:#x}", result);

            // Small delay
            for _ in 0..1000 { core::hint::spin_loop(); }

            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                log::info!("[a2-smp] rpi3: AP {} started via PSCI!", cpu_id);
            }
        }
    } else {
        log::info!("[a2-smp] rpi3: PSCI not available, using spin-table");
    }

    // Direct Trusted Mailbox test: write GO state to TF-A Trusted Mailbox
    // This bypasses the PSCI CPU_ON call to directly signal the cores
    #[cfg(target_arch = "aarch64")]
    {
        const TM_ENTRYPOINT: u64 = 0x10000000;
        const TM_HOLD_BASE: u64 = 0x10000008; // CPU0 at +0, CPU1 at +8, etc.
        const TM_STATE_GO: u64 = 1;

        // First set the entry point
        unsafe {
            core::ptr::write_volatile(TM_ENTRYPOINT as *mut u64, AP_BOOT_DEST_PA as u64);
            core::arch::asm!("dsb sy", options(nostack, preserves_flags));
            let entry_readback: u64 = core::ptr::read_volatile(TM_ENTRYPOINT as *const u64);
            log::info!("[a2-smp] TM: entry@{:#x}={:#x} (readback)", TM_ENTRYPOINT, entry_readback);
        }

        for cpu_id in 1..num_cpus {
            let hold_addr = TM_HOLD_BASE + (cpu_id as u64) * 8;
            unsafe {
                // Write GO state to signal core to jump to entry
                core::ptr::write_volatile(hold_addr as *mut u64, TM_STATE_GO);
                core::arch::asm!("dsb sy", "sev", options(nostack, preserves_flags));
                let state_readback: u64 = core::ptr::read_volatile(hold_addr as *const u64);
                log::info!("[a2-smp] TM: core{} hold@{:#x}={:#x} (readback)", cpu_id, hold_addr, state_readback);
            }
        }

        // Small delay
        for _ in 0..1000 { core::hint::spin_loop(); }

        // Check if APs started
        let mut ap_started = false;
        for cpu_id in 1..num_cpus {
            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                log::info!("[a2-smp] TM: AP {} started via Trusted Mailbox!", cpu_id);
                ap_started = true;
            }
        }
        if !ap_started {
            log::info!("[a2-smp] TM: no APs started via Trusted Mailbox, trying spin-table");
        }
    }

    // Fall back to spin-table if PSCI didn't work
    for cpu_id in 1..num_cpus {
        let release_addr = match get_cpu_release_addr(cpu_id) {
            Some(addr) => addr,
            None => {
                #[cfg(target_arch = "aarch64")]
                log::info!("[a2-smp] rpi3: no release addr for CPU {}", cpu_id);
                continue;
            }
        };
        #[cfg(target_arch = "aarch64")]
        log::info!("[a2-smp] rpi3: CPU {} spin-table@{:#x}", cpu_id, release_addr);

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
            let base_va = crate::arch::bcm2836_irq::local_ic_base_va();
            let mailbox_offset = CORE1_MAILBOX3_SET + 16 * (cpu_id as usize - 1);
            unsafe {
                core::ptr::write_volatile((base_va + mailbox_offset) as *mut u32, ap_entry_paddr as u32);
                core::arch::asm!("dsb sy", "sev", options(nostack, preserves_flags));
                let readback: u32 = core::ptr::read_volatile((base_va + mailbox_offset) as *const u32);
                log::info!("[a2-smp] rpi3: wrote mailbox@{:#x}={:#x}, readback={:#x}", mailbox_offset, ap_entry_paddr as u32, readback);
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            unsafe {
                core::ptr::write_volatile(release_addr as *mut u64, ap_entry_paddr);
                core::arch::asm!("dsb ish", "sev", options(nostack, preserves_flags));
                let readback: u64 = core::ptr::read_volatile(release_addr as *const u64);
                log::info!("[a2-smp] rpi3: spin-table@{:#x} wrote={:#x} readback={:#x}", release_addr, ap_entry_paddr, readback);
            }
        }

        for _ in 0..50000 {
            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                log::info!("[a2-smp] rpi3: AP {} started via spin-table!", cpu_id);
                break;
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    log::info!("[a2-smp] rpi3: SMP bringup done");
}
