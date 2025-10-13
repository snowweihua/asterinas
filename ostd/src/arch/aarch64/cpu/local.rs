// SPDX-License-Identifier: MPL-2.0

//! Architecture dependent CPU-local information utilities.

pub(crate) fn get_base() -> u64 {
    let mut base_address: u64;
    unsafe {
        // mrs - Move to Register from System register
        // Read TPIDR_EL1 into the base_address variable.
        core::arch::asm!("mrs {}, tpidr_el1", out(reg) base_address);
    }
    base_address as usize
}
