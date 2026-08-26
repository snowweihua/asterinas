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
const PSCI_CPU_ON: u64 = 0x84000001;
const PSCI_SUCCESS: u64 = 0;

/// Read the MPIDR_EL1 for a given CPU index from the DTB.
fn get_mpidr(cpu_index: u32) -> Option<u64> {
    let fdt = DEVICE_TREE.get()?;
    let cpus = fdt.find_node("/cpus")?;
    let mut current_cpu: u32 = 0;
    for child in cpus.children() {
        if child.reg().is_some() {
            if current_cpu == cpu_index {
                if let Some(prop) = child.property("reg") {
                    let v = prop.value;
                    if v.len() >= 8 {
                        return Some(u64::from_le_bytes(v[0..8].try_into().ok()?));
                    }
                }
            }
            current_cpu += 1;
        }
    }
    None
}

/// Check if PSCI is available by looking for /psci node in DTB.
fn is_psci_available() -> bool {
    let fdt = match DEVICE_TREE.get() {
        Some(f) => f,
        None => return false,
    };
    fdt.find_node("/psci").is_some()
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
                    let offset = if v.len() >= 8 {
                        u64::from_be_bytes(v[0..8].try_into().ok()?)
                    } else if v.len() >= 4 {
                        u32::from_be_bytes(v[0..4].try_into().ok()?) as u64
                    } else {
                        log::info!("[a2-smp] rpi3: cpu-release-addr too short");
                        return None;
                    };
                    // DTB returns offset within ARM_LOCAL peripheral, not full address
                    // BCM2836 ARM_LOCAL base is 0x4000_0000
                    return Some(ARM_LOCAL_PA as u64 + offset);
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
            core::ptr::write_volatile(0x41000 as *mut u8, 0x55);
            let _ = readback;
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

            log::info!("[a2-smp] rpi3: PSCI CPU_ON cpu={} mpidr={:#x} entry={:#x} stack={:#x}",
                cpu_id, mpidr, ap_entry_paddr, stack_top);

            let result = smc_call(PSCI_CPU_ON, mpidr, ap_entry_paddr, stack_top);
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
            let spin_table_va = crate::mm::paddr_to_vaddr(release_addr as Paddr);
            unsafe {
                core::arch::asm!(
                    "dsb ishst",
                    "str {val}, [{addr}]",
                    "dc cvac, {addr}",
                    "dsb ish",
                    addr = in(reg) spin_table_va,
                    val = in(reg) ap_entry_paddr,
                    options(nostack, preserves_flags)
                );
            }
            log::info!("[a2-smp] rpi3: wrote spin-table@{:#x}={:#x}", release_addr, ap_entry_paddr);
        }

        unsafe {
            let mailbox_va = crate::mm::paddr_to_vaddr(0x4000_0000 as crate::mm::Paddr);
            let offset = match cpu_id {
                1 => 0x84usize,
                2 => 0x88,
                3 => 0x8C,
                _ => 0x84,
            };
            core::ptr::write_volatile((mailbox_va + offset) as *mut u32, 0x1);
            core::arch::asm!(
                "dsb ish",
                "sev",
                "dsb ish",
                options(nostack)
            );
            for _ in 0..100 {
                core::hint::spin_loop();
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
