// SPDX-License-Identifier: MPL-2.0

//! The standard library for Asterinas and other Rust OSes.
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(btree_cursors)]
#![feature(const_ptr_sub_ptr)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(coroutines)]
#![feature(fn_traits)]
#![feature(iter_advance_by)]
#![feature(iter_from_coroutine)]
#![feature(let_chains)]
#![feature(linkage)]
#![feature(macro_metavar_expr)]
#![feature(min_specialization)]
#![feature(negative_impls)]
#![feature(ptr_metadata)]
#![feature(ptr_sub_ptr)]
#![feature(sync_unsafe_cell)]
#![feature(trait_upcasting)]
#![feature(unbounded_shifts)]
#![expect(internal_features)]
#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

extern crate alloc;

#[cfg(target_arch = "x86_64")]
#[path = "arch/x86/mod.rs"]
pub mod arch;
#[cfg(target_arch = "riscv64")]
#[path = "arch/riscv/mod.rs"]
pub mod arch;
#[cfg(target_arch = "loongarch64")]
#[path = "arch/loongarch/mod.rs"]
pub mod arch;
#[cfg(target_arch = "aarch64")]
#[path = "arch/aarch64/mod.rs"]
pub mod arch;
pub mod boot;
pub mod bus;
pub mod console;
pub mod cpu;
mod error;
pub mod io;
pub mod irq;
pub mod logger;
pub mod mm;
pub mod panic;
pub mod prelude;
pub mod smp;
pub mod sync;
pub mod task;
pub mod timer;
pub mod user;
pub mod util;

#[cfg(feature = "coverage")]
mod coverage;

use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "aarch64")]
use crate::arch::boot::pl011_puts;

/// Write a single character to the RPi3 mini-UART for debug markers.
/// Only usable on real RPi3 hardware (the mini-UART is at a fixed PA).
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn early_marker(ch: u8) {
    unsafe {
        core::arch::asm!(
            "movz x28, #0x3F21, lsl #16",
            "movk x28, #0x5040",
            "str w27, [x28]",
            in("w27") ch as u32,
            out("x28") _,
            options(nostack),
        );
    }
}

pub use ostd_macros::{
    global_frame_allocator, global_heap_allocator, global_heap_allocator_slot_map, main,
    panic_handler,
};
pub use ostd_pod::Pod;

pub use self::{error::Error, prelude::Result};

/// Initializes OSTD.
///
/// This function represents the first phase booting up the system. It makes
/// all functionalities of OSTD available after the call.
///
/// # Safety
///
/// This function should be called only once and only on the BSP.
//
// TODO: We need to refactor this function to make it more modular and
// make inter-initialization-dependencies more clear and reduce usages of
// boot stage only global variables.
#[doc(hidden)]
unsafe fn init() {
    unsafe { crate::arch::boot::pl011_puts(b"[init.0] start\n"); }
    arch::enable_cpu_features();
    unsafe { crate::arch::boot::pl011_puts(b"[init.1] enable_cpu_features done\n"); }

    // SAFETY: This function is called only once, before `allocator::init`
    // and after memory regions are initialized.
    unsafe { crate::arch::boot::pl011_puts(b"[init.1b] before init_early_allocator\n"); }
    unsafe { mm::frame::allocator::init_early_allocator() };
    unsafe { crate::arch::boot::pl011_puts(b"[init.2] init_early_allocator done\n"); }

    unsafe { crate::arch::boot::pl011_puts(b"[init.3] before serial::init\n"); }
    #[cfg(target_arch = "x86_64")]
    arch::if_tdx_enabled!({
    } else {
        arch::serial::init();
    });
    #[cfg(not(target_arch = "x86_64"))]
    arch::serial::init();
    unsafe { crate::arch::boot::pl011_puts(b"[init.4] after serial::init\n"); }

    unsafe { crate::arch::boot::pl011_puts(b"[init.5] before logger::init\n"); }
    logger::init();
    unsafe { crate::arch::boot::pl011_puts(b"[init.6] after logger::init\n"); }

    // SAFETY:
    // 1. They are only called once in the boot context of the BSP.
    // 2. The number of CPUs are available because ACPI has been initialized.
    // 3. No CPU-local objects have been accessed yet.
    unsafe { crate::arch::boot::pl011_puts(b"[init.7] before cpu::init_on_bsp\n"); }
    unsafe { cpu::init_on_bsp() };
    unsafe { crate::arch::boot::pl011_puts(b"[init.8] after cpu::init_on_bsp\n"); }

    let meta_pages = unsafe { mm::frame::meta::init() };
    unsafe { crate::arch::boot::pl011_puts(b"[init.9] after meta::init\n"); }

    // On AArch64, reserve a page for the kernel page table root BEFORE the
    // early allocator is retired.  This avoids calling the broken
    // alloc_frame_with() later (which crashes at `ret` on RPi3 hardware).
    #[cfg(target_arch = "aarch64")]
    {
        unsafe { crate::arch::boot::pl011_puts(b"[init.9b] before reserve_root_pt_page\n"); }
        mm::page_table::reserve_root_pt_page();
        unsafe { crate::arch::boot::pl011_puts(b"[init.9c] after reserve_root_pt_page\n"); }
    }

    // The frame allocator should be initialized immediately after the metadata
    // is initialized. Otherwise the boot page table can't allocate frames.
    // SAFETY: This function is called only once.
    unsafe { mm::frame::allocator::init() };
    unsafe { crate::arch::boot::pl011_puts(b"[init.A] after allocator::init\n"); }

    boot::init_after_heap();
    unsafe { crate::arch::boot::pl011_puts(b"[init.B] after init_after_heap\n"); }

    unsafe { crate::arch::boot::pl011_puts(b"[init.B1] before kspace::init\n"); }
    mm::kspace::init_kernel_page_table(meta_pages);
    unsafe { crate::arch::boot::pl011_puts(b"[init.C] after kspace::init\n"); }

    sync::init();
    unsafe { crate::arch::boot::pl011_puts(b"[init] after sync::init\n"); }

    mm::dma::init();
    unsafe { crate::arch::boot::pl011_puts(b"[init.dma] after dma::init\n"); }

    unsafe { arch::late_init_on_bsp() };
    unsafe { crate::arch::boot::pl011_puts(b"[init.4] after late_init_on_bsp\n"); }

    #[cfg(target_arch = "x86_64")]
    arch::if_tdx_enabled!({
        arch::serial::init();
    });

    if cfg!(target_arch = "aarch64") && crate::arch::board::BoardType::cached() == 2 {
        unsafe { crate::arch::boot::pl011_puts(b"[init.5] smp::init skipped on RPi3\n"); }
    } else {
        smp::init();
    }

    {
        unsafe { crate::arch::boot::pl011_puts(b"[init.5] before activate_kernel_page_table\n"); }
        unsafe {
            mm::kspace::activate_kernel_page_table();
        }
        unsafe { crate::arch::boot::pl011_puts(b"[init.5a] after activate_kernel_page_table\n"); }
    }

    unsafe { crate::arch::boot::pl011_puts(b"[init.6] before IN_BOOTSTRAP_CONTEXT store\n"); }
    IN_BOOTSTRAP_CONTEXT.store(false, Ordering::Relaxed);
    unsafe { crate::arch::boot::pl011_puts(b"[init.7] before enable_local_irq\n"); }

    arch::irq::enable_local();
    unsafe { crate::arch::boot::pl011_puts(b"[init.8] before invoke_ffi_init_funcs\n"); }

    invoke_ffi_init_funcs();
    unsafe { crate::arch::boot::pl011_puts(b"[init.9] after invoke_ffi_init_funcs\n"); }
}

/// Indicates whether the kernel is in bootstrap context.
pub(crate) static IN_BOOTSTRAP_CONTEXT: AtomicBool = AtomicBool::new(true);

/// Invoke the initialization functions defined in the FFI.
/// The component system uses this function to call the initialization functions of
/// the components.
fn invoke_ffi_init_funcs() {
    unsafe extern "C" {
        fn __ostd_main();
    }
    unsafe { crate::arch::boot::pl011_puts(b"[IFF] skip init_array, calling __ostd_main directly\n") };
    unsafe { __ostd_main() };
}

/// Simple unit tests for the ktest framework.
#[cfg(ktest)]
mod test {
    use crate::prelude::*;

    #[ktest]
    #[expect(clippy::eq_op)]
    fn trivial_assertion() {
        assert_eq!(0, 0);
    }

    #[ktest]
    #[should_panic]
    fn failing_assertion() {
        assert_eq!(0, 1);
    }

    #[ktest]
    #[should_panic(expected = "expected panic message")]
    fn expect_panic() {
        panic!("expected panic message");
    }
}

#[doc(hidden)]
pub mod ktest {
    //! The module re-exports everything from the [`ostd_test`] crate, as well
    //! as the test entry point macro.
    //!
    //! It is rather discouraged to use the definitions here directly. The
    //! `ktest` attribute is sufficient for all normal use cases.

    pub use ostd_macros::{test_main as main, test_panic_handler as panic_handler};
    pub use ostd_test::*;
}
