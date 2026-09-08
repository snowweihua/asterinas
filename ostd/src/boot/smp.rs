// SPDX-License-Identifier: MPL-2.0

//! Symmetric multiprocessing (SMP) boot support.

use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};

use spin::Once;

use crate::{
    arch::{boot::smp, boot::smp_rpi3, irq::HwCpuId},
    mm::{
        frame::{meta::KernelMeta, Segment},
        paddr_to_vaddr, FrameAllocOptions, HasPaddrRange, PAGE_SIZE,
    },
    sync::SpinLock,
    task::Task,
};

static AP_BOOT_INFO: Once<ApBootInfo> = Once::new();

const AP_BOOT_STACK_SIZE: usize = PAGE_SIZE * 64;

struct ApBootInfo {
    /// Raw boot information for each AP.
    per_ap_raw_info: Box<[PerApRawInfo]>,
    /// Boot information for each AP.
    #[expect(dead_code)]
    per_ap_info: Box<[PerApInfo]>,
}

struct PerApInfo {
    // TODO: When the AP starts up and begins executing tasks, the boot stack will
    // no longer be used, and the `Segment` can be deallocated (this problem also
    // exists in the boot processor, but the memory it occupies should be returned
    // to the frame allocator).
    #[expect(dead_code)]
    boot_stack_pages: Segment<KernelMeta>,
}

/// Raw boot information for APs.
///
/// This is "raw" information that the assembly code (run by APs at startup,
/// before ever entering the Rust entry point) will directly access. So the
/// layout is important. **Update the assembly code if the layout is changed!**
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct PerApRawInfo {
    pub(crate) stack_top: *mut u8,
    pub(crate) cpu_local: *mut u8,
}

// SAFETY: This information (i.e., the pointer addresses) can be shared safely
// among multiple threads. However, it is the responsibility of the user to
// ensure that the contained pointers are used safely.
unsafe impl Send for PerApRawInfo {}
unsafe impl Sync for PerApRawInfo {}

static HW_CPU_ID_MAP: SpinLock<BTreeMap<u32, HwCpuId>> = SpinLock::new(BTreeMap::new());

/// Boots all application processors.
///
/// This function should be called late in the system startup. The system must at
/// least ensure that the scheduler, ACPI table, memory allocation, and IPI module
/// have been initialized.
///
/// # Safety
///
/// This function can only be called in the boot context of the BSP where APs have
/// not yet been booted.
pub(crate) unsafe fn boot_all_aps() {
    // Mark the BSP as started.
    report_online_and_hw_cpu_id(crate::cpu::CpuId::bsp().as_usize().try_into().unwrap());

    let num_cpus = crate::cpu::num_cpus();

    if num_cpus == 1 {
        {
        }
        return;
    }

    // Debug output
    {
    }

    let mut per_ap_raw_info = Vec::with_capacity(num_cpus);
    let mut per_ap_info = Vec::with_capacity(num_cpus);

    for ap in 1..num_cpus {
        let boot_stack_pages = FrameAllocOptions::new()
            .zeroed(false)
            .alloc_segment_with(AP_BOOT_STACK_SIZE / PAGE_SIZE, |_| KernelMeta)
            .unwrap();

        per_ap_raw_info.push(PerApRawInfo {
            stack_top: paddr_to_vaddr(boot_stack_pages.end_paddr()) as *mut u8,
            cpu_local: paddr_to_vaddr(crate::cpu::local::get_ap(ap.try_into().unwrap())) as *mut u8,
        });
        per_ap_info.push(PerApInfo { boot_stack_pages });
    }

    assert!(!AP_BOOT_INFO.is_completed());
    AP_BOOT_INFO.call_once(move || ApBootInfo {
        per_ap_raw_info: per_ap_raw_info.into_boxed_slice(),
        per_ap_info: per_ap_info.into_boxed_slice(),
    });

    let info_ptr = AP_BOOT_INFO.get().unwrap().per_ap_raw_info.as_ptr();
    let pt_ptr = crate::mm::page_table::boot_pt::with_borrow(|pt| pt.root_address()).unwrap();

    let is_rpi3 = crate::arch::board::BoardType::cached() == 2;

    if is_rpi3 {
        unsafe { smp_rpi3::bringup_all_aps_rpi3(info_ptr, pt_ptr, num_cpus as u32) };
    } else {
        unsafe { smp::bringup_all_aps(info_ptr, pt_ptr, num_cpus as u32) };
    }

    wait_for_all_aps_started(num_cpus);
}

static AP_LATE_ENTRY: Once<fn()> = Once::new();

/// Registers the entry function for the application processor.
///
/// Once the entry function is registered, all the application processors
/// will jump to the entry function immediately.
pub fn register_ap_entry(entry: fn()) {
    AP_LATE_ENTRY.call_once(|| entry);
}

#[inline(always)]
unsafe fn ap_dram_ckpt(cpu_id: u32, stage: u32) {
    let pa = 0x68000usize + (cpu_id as usize) * 0x1000 + (stage as usize) * 4;
    let va = crate::mm::kspace::paddr_to_vaddr(pa);
    let val: u32 = 0xd0 + cpu_id;
    core::ptr::write_volatile(va as *mut u32, val);
}

#[unsafe(no_mangle)]
fn ap_early_entry(cpu_id: u32) -> ! {
    unsafe { ap_dram_ckpt(cpu_id, 0); }

    // NOTE: serial::send is intentionally NOT called on the AP here. It touches
    // shared/global UART state and has been observed to fault/corrupt on APs.

    // SAFETY: The safety is upheld by the caller.
    unsafe { crate::cpu::init_on_ap(cpu_id) };

    unsafe { ap_dram_ckpt(cpu_id, 1); }
    crate::arch::enable_cpu_features();

    // SAFETY: This function is called in the boot context of the AP.
    unsafe { ap_dram_ckpt(cpu_id, 2); }
    unsafe { crate::arch::trap::init() };

    // SAFETY: This function is only called once on this AP, after the BSP has
    // done the architecture-specific initialization.
    unsafe { ap_dram_ckpt(cpu_id, 3); }
    unsafe { crate::arch::init_on_ap() };

    unsafe { ap_dram_ckpt(cpu_id, 4); }
    crate::arch::irq::enable_local();

    // SAFETY: This function is only called once on this AP.
    unsafe { ap_dram_ckpt(cpu_id, 5); }
    unsafe { crate::mm::kspace::activate_kernel_page_table() };

    unsafe { ap_dram_ckpt(cpu_id, 6); }
    // Mark the AP as started.
    report_online_and_hw_cpu_id(cpu_id);

    let ap_late_entry = AP_LATE_ENTRY.wait();
    ap_late_entry();

    loop {
        crate::task::halt_cpu();
    }
}

pub(crate) fn report_online_and_hw_cpu_id(cpu_id: u32) {
    // There are no races because this method will only be called in the boot
    // context, where preemption won't occur.
    let hw_cpu_id = HwCpuId::read_current(&crate::task::disable_preempt());

    let old_val = HW_CPU_ID_MAP.lock().insert(cpu_id, hw_cpu_id);
    assert!(old_val.is_none());
}

fn wait_for_all_aps_started(num_cpus: usize) {
    fn is_all_aps_started(num_cpus: usize) -> bool {
        HW_CPU_ID_MAP.lock().len() == num_cpus
    }

    const TIMEOUT_ITERATIONS: usize = 500000;
    let mut iterations = 0;

    while !is_all_aps_started(num_cpus) {
        iterations += 1;
        if iterations > TIMEOUT_ITERATIONS {
            log::warn!("[a2-smp] TIMEOUT: waiting for {} CPUs, only {} online after ~{} iterations",
                num_cpus, HW_CPU_ID_MAP.lock().len(), TIMEOUT_ITERATIONS);
            break;
        }
        core::hint::spin_loop();
    }

    let online = HW_CPU_ID_MAP.lock().len();
    if online < num_cpus {
        log::warn!("[a2-smp] Only {}/{} CPUs online", online, num_cpus);
    }
}

/// Constructs a boxed slice that maps [`CpuId`] to [`HwCpuId`].
///
/// # Panics
///
/// This method will panic if it is called either before all APs have booted or more than once.
///
/// [`CpuId`]: crate::cpu::CpuId
pub(crate) fn construct_hw_cpu_id_mapping() -> Box<[HwCpuId]> {
    let mut hw_cpu_id_map = HW_CPU_ID_MAP.lock();
    assert_eq!(hw_cpu_id_map.len(), crate::cpu::num_cpus());

    let result = hw_cpu_id_map
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .into_boxed_slice();
    hw_cpu_id_map.clear();

    result
}
