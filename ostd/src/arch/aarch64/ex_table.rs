// SPDX-License-Identifier: MPL-2.0

//! AArch64 exception table for fallible kernel accesses to user-space memory.
//!
//! When EL1 (kernel) code takes a Data Abort while accessing a user-space
//! address that cannot be resolved by the VMAR page-fault handler, the
//! `sync_exception_current` handler searches this table for the faulting PC.
//! If found, it redirects ELR_EL1 to the corresponding fixup label, allowing
//! the faulting function to return a failure code rather than hanging.
//!
//! Assembly files register entries by placing pairs `(fault_pc, fixup_pc)`
//! in the `.ex_table` linker section:
//!
//! ```asm
//! .pushsection .ex_table, "a"
//!     .align 8
//!     .quad .Lfault_label     /* instruction that may fault */
//!     .quad .Lfixup_label     /* recovery: sets return value and returns */
//! .popsection
//! ```

use crate::prelude::Vaddr;

#[repr(C)]
struct ExTableItem {
    inst_addr: Vaddr,
    recovery_inst_addr: Vaddr,
}

unsafe extern "C" {
    fn __ex_table();
    fn __ex_table_end();
}

/// Searches the exception table for a recovery address matching `inst_addr`.
///
/// Returns `Some(recovery_addr)` if found, `None` otherwise.
pub(crate) fn find_recovery_inst_addr(inst_addr: Vaddr) -> Option<Vaddr> {
    let table_size = (__ex_table_end as usize - __ex_table as usize) / size_of::<ExTableItem>();
    // SAFETY: `__ex_table` is a linker-defined section filled with `ExTableItem`
    // pairs by the assembly fallible-copy routines.
    let ex_table =
        unsafe { core::slice::from_raw_parts(__ex_table as *const ExTableItem, table_size) };
    for item in ex_table {
        // Skip zero-filled padding entries that the linker inserts between
        // input sections to satisfy alignment requirements.
        if item.inst_addr == 0 {
            continue;
        }
        if item.inst_addr == inst_addr {
            return Some(item.recovery_inst_addr);
        }
    }
    None
}
