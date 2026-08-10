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


    let ap_entry_paddr = ap_boot_entry as usize as u64;

    unsafe {
        __ap_boot_info_array_pointer = info_ptr;
        __boot_page_table_pointer = pt_ptr as u64;
    }


    for cpu_id in 1..num_cpus {
        let mpidr = get_mpidr(cpu_id);
        let info = unsafe { &*info_ptr.add(cpu_id as usize - 1) };

        let stack_top = info.stack_top as u64;

        let result = psci_call(PSCI_CPU_ON, mpidr, ap_entry_paddr, stack_top);

        if result != PSCI_SUCCESS {
            log::warn!("PSCI CPU_ON for CPU {} returned {:x}", cpu_id, result);
        }
    }

}
