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
    #[cfg(not(target_arch = "aarch64"))]
    arch::enable_cpu_features();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i-skip] skipping enable_cpu_features");
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i0] after enable_cpu_features");

    // SAFETY: This function is called only once, before `allocator::init`
    // and after memory regions are initialized.
    unsafe { mm::frame::allocator::init_early_allocator() };

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i1] after init_early_allocator");

    #[cfg(target_arch = "x86_64")]
    arch::if_tdx_enabled!({
    } else {
        arch::serial::init();
    });
    #[cfg(not(target_arch = "x86_64"))]
    arch::serial::init();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i2] after serial::init");

    logger::init();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i3] after logger::init");

    // SAFETY:
    // 1. They are only called once in the boot context of the BSP.
    // 2. The number of CPUs are available because ACPI has been initialized.
    // 3. No CPU-local objects have been accessed yet.
    unsafe { cpu::init_on_bsp() };
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i4] after cpu::init_on_bsp");

    // SAFETY: We are on the BSP and APs are not yet started.
    let meta_pages = unsafe { mm::frame::meta::init() };
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i5] after mm::frame::meta::init");
    // The frame allocator should be initialized immediately after the metadata
    // is initialized. Otherwise the boot page table can't allocate frames.
    // SAFETY: This function is called only once.
    #[cfg(not(target_arch = "aarch64"))]
    unsafe { mm::frame::allocator::init() };

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[fa-skip] skip allocator::init on aarch64 phase-1");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i18] before kspace::init_kernel_page_table");
    #[cfg(not(target_arch = "aarch64"))]
    mm::kspace::init_kernel_page_table(meta_pages);
    #[cfg(target_arch = "aarch64")]
    let _ = meta_pages;
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i19] after kspace::init_kernel_page_table");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i20] before sync::init");
    sync::init();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i21] after sync::init");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i22] before boot::init_after_heap");
    #[cfg(not(target_arch = "aarch64"))]
    boot::init_after_heap();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i23] after boot::init_after_heap");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i24] before mm::dma::init");
    mm::dma::init();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i25] after mm::dma::init");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i26] before arch::late_init_on_bsp");
    unsafe { arch::late_init_on_bsp() };
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i27] after arch::late_init_on_bsp");

    #[cfg(target_arch = "x86_64")]
    arch::if_tdx_enabled!({
        arch::serial::init();
    });

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i28] before smp::init");
    #[cfg(not(target_arch = "aarch64"))]
    smp::init();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i29] after smp::init");

    // SAFETY: This function is called only once on the BSP.
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i30] before activate_kernel_page_table");
    #[cfg(not(target_arch = "aarch64"))]
    unsafe {
        mm::kspace::activate_kernel_page_table();
    }
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i31] after activate_kernel_page_table");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i32] before irq enable");
    arch::irq::enable_local();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i33] after irq enable");

    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i34] before invoke_ffi_init_funcs");
    #[cfg(not(target_arch = "aarch64"))]
    invoke_ffi_init_funcs();
    #[cfg(target_arch = "aarch64")]
    crate::early_println!("[i35] after invoke_ffi_init_funcs");

    IN_BOOTSTRAP_CONTEXT.store(false, Ordering::Relaxed);
}

/// Indicates whether the kernel is in bootstrap context.
pub(crate) static IN_BOOTSTRAP_CONTEXT: AtomicBool = AtomicBool::new(true);

/// Invoke the initialization functions defined in the FFI.
/// The component system uses this function to call the initialization functions of
/// the components.
fn invoke_ffi_init_funcs() {
    unsafe extern "C" {
        fn __sinit_array();
        fn __einit_array();
    }
    let call_len = (__einit_array as usize - __sinit_array as usize) / 8;
    for i in 0..call_len {
        unsafe {
            let function = (__sinit_array as usize + 8 * i) as *const fn();
            (*function)();
        }
    }
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
