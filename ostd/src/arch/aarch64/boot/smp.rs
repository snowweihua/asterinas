// SPDX-License-Identifier: MPL-2.0

//! Multiprocessor Boot Support

use super::DEVICE_TREE;
use crate::{boot::smp::PerApRawInfo, mm::Paddr};

pub(crate) fn count_processors() -> Option<u32> {
    let fdt = DEVICE_TREE.get()?;
    let cpus = fdt.find_node("/cpus")?;
    let mut count = 0u32;
    for child in cpus.children() {
        if child.reg().is_some() {
            count += 1;
        }
    }
    if count == 0 {
        count = 1;
    }
    if crate::arch::board::BoardType::cached() == 2 && !super::smp_rpi3::psci_usable() {
        count = 1;
    }
    Some(count)
}

const PSCI_CPU_ON: u64 = 0x84000001;
const PSCI_SUCCESS: u64 = 0;

unsafe extern "C" {
    fn ap_boot_entry();
    static mut __ap_boot_info_array_pointer: *const PerApRawInfo;
    static mut __boot_page_table_pointer: u64;
}

fn get_mpidr(cpu_index: u32) -> u64 {
    let fdt = DEVICE_TREE.get().expect("Device tree not initialized");
    let cpus = fdt.find_node("/cpus").expect("/cpus node not found");

    let mut current_cpu: u32 = 0;
    for child in cpus.children() {
        if child.reg().is_some() {
            if current_cpu == cpu_index {
                if let Some(prop) = child.property("reg") {
                    let reg_values = prop.value;
                    if reg_values.len() >= 8 {
                        return u64::from_le_bytes(reg_values[0..8].try_into().unwrap());
                    }
                }
            }
            current_cpu += 1;
        }
    }
    0
}

fn psci_call(function_id: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "hvc #0",
            inout("x0") function_id => result,
            in("x1") arg0,
            in("x2") arg1,
            in("x3") arg2,
        );
    }
    result
}

pub(crate) unsafe fn bringup_all_aps(info_ptr: *const PerApRawInfo, pt_ptr: Paddr, num_cpus: u32) {
    if crate::arch::board::BoardType::cached() == 2 {
        unsafe { super::smp_rpi3::bringup_all_aps_rpi3(info_ptr, pt_ptr, num_cpus) };
    } else {
        unsafe { bringup_all_aps_virt(info_ptr, pt_ptr, num_cpus) };
    }
}

/// Scratch PA holding the AP info array for MMU-off readers.
///
/// The generic array lives on the heap, which the boot tables do not map.
/// Copying the entries to this identity-mapped scratch area lets the AP stub
/// (and the PSCI context ID) use physical addresses that work both with the
/// MMU off and through the TTBR0 identity map.
pub(crate) const AP_INFO_SCRATCH_PA: usize = 0x5_0020;

/// TEMP-HW-DEBUG: scratch slot carrying the runtime KPT root PA from the BSP
/// to APs. The AP-side `Once` read faults with `ldxr` on HW, so the BSP
/// publishes the root explicitly through the proven scratch channel
/// (same linear-alias + `dc cvac` pattern as the working SP/TPIDR path).
/// Revert-or-promote after validation.
pub(crate) const KPT_ROOT_SCRATCH_PA: usize = 0x5_0060;

/// Pushes the AP boot globals to the point of coherency.
///
/// The AP stub reads these with the MMU off, bypassing the cache, so a
/// dirty cache line here would make APs observe stale values (e.g. a zero
/// page-table root, which faults every AP immediately).
pub(crate) unsafe fn flush_ap_boot_globals() {
    unsafe extern "C" {
        static __ap_boot_info_array_pointer: u64;
        static __boot_page_table_pointer: u64;
    }
    unsafe {
        let base = &__ap_boot_info_array_pointer as *const u64 as usize & !63;
        for offset in (0..320).step_by(64) {
            core::arch::asm!(
                "dc cvac, {addr}",
                addr = in(reg) base + offset,
                options(nostack, preserves_flags)
            );
        }
        core::arch::asm!("dsb ish", options(nostack, preserves_flags));
    }
}

unsafe extern "C" {
    static boot_l4pt: u8;
}

/// Physical address of the boot page-table root.
///
/// APs need identity, kernel-window and linear mappings from a single root,
/// which only the boot tables provide. The active root at bringup time is
/// the kernel page table (high half only) and would strand APs after they
/// enable the MMU.
pub(crate) fn boot_root_paddr() -> Paddr {
    let vma = unsafe { &boot_l4pt as *const u8 as usize };
    let offset = crate::mm::kspace::kernel_loaded_offset();
    if vma >= offset {
        vma - offset
    } else {
        vma
    }
}

pub(crate) unsafe fn publish_ap_info_to_scratch(info_ptr: *const PerApRawInfo, num_cpus: u32) {
    for i in 0..(num_cpus as usize).saturating_sub(1) {
        let entry = unsafe { &*info_ptr.add(i) };
        // Linear alias: the BSP runs with the MMU on (kernel page table).
        let dst = crate::mm::kspace::paddr_to_vaddr(AP_INFO_SCRATCH_PA + i * 16) as *mut u64;
        unsafe {
            core::ptr::write_volatile(dst, entry.stack_top() as u64);
            core::ptr::write_volatile(dst.byte_add(8), entry.cpu_local() as u64);
            core::arch::asm!(
                "dc cvac, {addr}",
                addr = in(reg) dst as usize,
                options(nostack, preserves_flags)
            );
        }
    }
    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
}

pub(crate) unsafe fn publish_kpt_root_to_scratch(root: Paddr) {
    let dst = crate::mm::kspace::paddr_to_vaddr(KPT_ROOT_SCRATCH_PA) as *mut u64;
    unsafe {
        core::ptr::write_volatile(dst, root as u64);
        core::arch::asm!(
            "dc cvac, {addr}",
            addr = in(reg) dst as usize,
            options(nostack, preserves_flags)
        );
        core::arch::asm!("dsb ish", options(nostack, preserves_flags));
    }
}

pub(crate) unsafe fn bringup_all_aps_virt(info_ptr: *const PerApRawInfo, _pt_ptr: Paddr, num_cpus: u32) {
    log::info!("[a2-smp] PSCI: START bringup_all_aps num_cpus={}", num_cpus);

    unsafe {
        publish_ap_info_to_scratch(info_ptr, num_cpus);
        if let Some(root) = crate::mm::kspace::kernel_page_table_root_paddr() {
            publish_kpt_root_to_scratch(root);
        }
        __ap_boot_info_array_pointer = AP_INFO_SCRATCH_PA as *const PerApRawInfo;
        __boot_page_table_pointer = boot_root_paddr() as u64;
        flush_ap_boot_globals();
    }

    let ap_entry_paddr =
        (ap_boot_entry as usize - crate::mm::kspace::kernel_loaded_offset()) as u64;
    log::info!("[a2-smp] PSCI: ap_entry_paddr=0x{:016x}", ap_entry_paddr);

    for cpu_id in 1..num_cpus {
        let mpidr = get_mpidr(cpu_id);
        let info_pa = AP_INFO_SCRATCH_PA as u64 + (cpu_id as u64 - 1) * 16;

        log::info!("[a2-smp] PSCI: calling CPU_ON cpu_id={} mpidr=0x{:016x}", cpu_id, mpidr);

        let result = psci_call(PSCI_CPU_ON, mpidr, ap_entry_paddr, info_pa);

        log::info!("[a2-smp] PSCI: cpu_id={} result=0x{:016x}", cpu_id, result);

        if result != PSCI_SUCCESS {
            log::warn!("PSCI CPU_ON for CPU {} returned {:x}", cpu_id, result);
        }
    }

    log::info!("[a2-smp] PSCI: DONE bringup_all_aps");
}
