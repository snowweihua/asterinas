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

pub(crate) unsafe fn bringup_all_aps(
    _info_ptr: *const PerApRawInfo,
    _pr_ptr: Paddr,
    _num_cpus: u32,
) {
}
