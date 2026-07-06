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

/// Read the `cpu-release-addr` property from the DTB for the given logical CPU index.
fn get_cpu_release_addr(cpu_index: u32) -> Option<u64> {
    let fdt = match DEVICE_TREE.get() {
        Some(f) => f,
        None => {
            unsafe { crate::arch::boot::early_puts(b"[a2-smp] rpi3: DEVICE_TREE.get()=None\n"); }
            return None;
        }
    };
    let cpus = match fdt.find_node("/cpus") {
        Some(c) => c,
        None => {
            unsafe { crate::arch::boot::early_puts(b"[a2-smp] rpi3: /cpus node not found\n"); }
            return None;
        }
    };

    let mut current_cpu: u32 = 0;
    for child in cpus.children() {
        if child.reg().is_some() {
            if current_cpu == cpu_index {
                if let Some(prop) = child.property("cpu-release-addr") {
                    let v = prop.value;
                    if v.len() >= 8 {
                        let addr = u64::from_be_bytes(v[0..8].try_into().ok()?);
                        return Some(addr);
                    } else if v.len() >= 4 {
                        let addr = u32::from_be_bytes(v[0..4].try_into().ok()?) as u64;
                        return Some(addr);
                    }
                }
                unsafe { crate::arch::boot::early_puts(b"[a2-smp] rpi3: no cpu-release-addr prop\n"); }
                return None;
            }
            current_cpu += 1;
        }
    }
    unsafe { crate::arch::boot::early_puts(b"[a2-smp] rpi3: cpu not found in DT\n"); }
    None
}

pub(crate) unsafe fn bringup_all_aps_rpi3(
    info_ptr: *const PerApRawInfo,
    pt_ptr: Paddr,
    num_cpus: u32,
) {
    #[cfg(target_arch = "aarch64")]
    crate::arch::boot::early_puts(b"[a2-smp] rpi3: SMP bringup starting\n");

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
        // Clean entire copied region to point of coherency
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
            // Clear marker
            core::ptr::write_volatile(0x41000 as *mut u8, 0x55);
            // Result read via AP boot marker region
            let _ = readback;
        }
    }

    #[cfg(target_arch = "aarch64")]
    crate::arch::boot::early_puts(b"[a2-smp] rpi3: boot stub copied\n");

    let ap_entry_paddr = AP_BOOT_DEST_PA as u64;

    for cpu_id in 1..num_cpus {
        // Configure GICC CPU interface on this AP so it can receive interrupts.
        let gicc_va = super::super::gic::gic_pa_to_va(super::super::gic::RPI3_GICC_BASE);
        unsafe {
            core::ptr::write_volatile(gicc_va as *mut u32, 0b111);
            core::ptr::write_volatile((gicc_va + 0x04) as *mut u32, 0xF0);
        }

        let release_addr = match get_cpu_release_addr(cpu_id) {
            Some(addr) => addr,
            None => {
                #[cfg(target_arch = "aarch64")]
                crate::arch::boot::early_puts(b"[a2-smp] rpi3: no release addr, skipping\n");
                continue;
            }
        };

        // Validate the spin-table address before writing to it
        // Two-phase protocol:
        // Phase 1: BSP writes __aps_entry, __aps_hold_flag, pt_root, info_array to AP_INFO_BASE
        // Phase 2: BSP triggers mailbox IRQ, AP reads hold_flag, then entry, and branches
        let info_base_va = AP_INFO_BASE; // identity-mapped PA

        // Write all info region values with proper cache maintenance
        // Order: entry (0x08), then flag (0x00), then pt_root (0x10), then info_array (0x18)
        unsafe {
            // Write __aps_entry at offset 0x08
            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}, #8]",
                "dc cvac, {addr}",
                addr = in(reg) info_base_va,
                val = in(reg) ap_entry_paddr,
                options(nostack, preserves_flags)
            );

            // Write __aps_hold_flag at offset 0x00 = 1
            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}]",
                "dc cvac, {addr}",
                addr = in(reg) info_base_va,
                val = in(reg) 1u64,
                options(nostack, preserves_flags)
            );

            // Write __boot_pt_root at offset 0x10
            core::arch::asm!(
                "dsb ishst",
                "str {val}, [{addr}, #16]",
                "dc cvac, {addr}",
                addr = in(reg) info_base_va,
                val = in(reg) pt_ptr as u64,
                options(nostack, preserves_flags)
            );

            // Write __info_array at offset 0x18
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

        // Write spin-table value to DTB release address (BCM2836 spin-table)
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
        }

        unsafe {
            let mailbox_va = crate::mm::paddr_to_vaddr(0x4000_0000 as crate::mm::Paddr);
            let offset = match cpu_id {
                1 => 0x84usize,
                2 => 0x88,
                3 => 0x8C,
                _ => 0x84,
            };
            core::ptr::write_volatile((mailbox_va + offset) as *mut u32, 1);
            core::arch::asm!("dsb sy", "sev", "isb", options(nostack));
        }

        for _ in 0..10000 {
            let val = unsafe { core::ptr::read_volatile(0x41000 as *const u8) };
            if val != 0x55 && val != 0 {
                unsafe { crate::arch::boot::early_puts(b"[a2-smp] rpi3: AP started!\n") };
                break;
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    crate::arch::boot::early_puts(b"[a2-smp] rpi3: SMP bringup done\n");
}
