// SPDX-License-Identifier: MPL-2.0

//! The AArch64 `rt_sigreturn` trampoline.
//!
//! AArch64 libc does not use `SA_RESTORER`, so the kernel must supply the
//! signal-return trampoline. Linux provides it via the vDSO; Asterinas has no
//! AArch64 vDSO, so a minimal executable trampoline page is mapped per process
//! (see `process::program_loader::elf`).

use alloc::sync::Arc;

use ostd::mm::{PAGE_SIZE, VmIo, VmReader};
use spin::Once;

use crate::vm::page_cache::{Vmo, VmoOptions};

/// The trampoline instructions: `mov x8, #139; svc #0; brk #0`.
const SIGRETURN_TRAMPOLINE: [u8; 12] = [
    0x68, 0x11, 0x80, 0xd2, // mov x8, #139 (`__NR_rt_sigreturn`)
    0x01, 0x00, 0x00, 0xd4, // svc #0
    0x00, 0x00, 0x20, 0xd4, // brk #0
];

/// Returns the VMO holding the `rt_sigreturn` trampoline.
pub(crate) fn trampoline_vmo() -> Arc<Vmo> {
    static VMO: Once<Arc<Vmo>> = Once::new();
    VMO.call_once(|| {
        let vmo = VmoOptions::new(PAGE_SIZE).alloc().unwrap();
        let mut reader = VmReader::from(&SIGRETURN_TRAMPOLINE[..]).to_fallible();
        vmo.write(0, &mut reader).unwrap();
        vmo
    })
    .clone()
}
