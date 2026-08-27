// SPDX-License-Identifier: MPL-2.0

//! The RISC-V boot module defines the entrypoints of Asterinas.

pub mod smp;
pub mod smp_rpi3;

use core::arch::global_asm;

use fdt::Fdt;

use crate::boot::SimpleOnce as Once;

use crate::{
    boot::{
        memory_region::{MemoryRegion, MemoryRegionArray, MemoryRegionType},
        BootloaderAcpiArg, BootloaderFramebufferArg,
    },
    mm::paddr_to_vaddr,
};

global_asm!(include_str!("boot.S"));
global_asm!(".section .ap_boot,\"axG\",@progbits,8", include_str!("ap_boot.S"));


/// The Flattened Device Tree of the platform.
pub static DEVICE_TREE: Once<Fdt> = Once::new();
static DEVICE_TREE_REGION: Once<(usize, usize)> = Once::new();

const QEMU_VIRT_RAM_BASE: usize = 0x4000_0000;
const QEMU_VIRT_RAM_SCAN_SIZE: usize = 512 * 1024 * 1024;
const QEMU_LOADER_DTB_PADDR: usize = 0x4700_0000;
const FDT_MAX_TOTAL_SIZE: usize = 2 * 1024 * 1024;
const FDT_MAGIC_BE: [u8; 4] = [0xd0, 0x0d, 0xfe, 0xed];

/// A high-half, page-table-resident copy of the DTB.
///
/// Accessing the DTB through the boot-time raw physical pointer is fragile:
/// the fdt crate returns a `&[u8]` and later methods can dereference through it
/// with assumptions that are safer to satisfy with a normal `static` slice.
static mut DTB_COPY: [u8; FDT_MAX_TOTAL_SIZE] = [0; FDT_MAX_TOTAL_SIZE];

pub fn kernel_physical_base(kernel_start: usize, kernel_loaded_offset: usize) -> usize {
    crate::arch::board::dram_base() + (kernel_start - kernel_loaded_offset)
}

fn parse_bootloader_name() -> &'static str {
    "Unknown"
}

fn parse_kernel_commandline() -> &'static str {
    if crate::arch::board::BoardType::cached() != 2 {
        return "init=/init console=ttyAMA0";
    }
    DEVICE_TREE.get().unwrap().chosen().bootargs().unwrap_or("")
}

/// Hardcoded initramfs location used by QEMU virt.
// test/build/initramfs.cpio: 44051456 bytes = 0x2a02c00
const QEMU_INITRAMFS_START: usize = 0x5800_0000;
const QEMU_INITRAMFS_END: usize = 0x5aa0_2c00;

fn parse_initramfs() -> Option<&'static [u8]> {
    if crate::arch::board::BoardType::cached() != 2 {
        return Some(unsafe {
            core::slice::from_raw_parts(
                paddr_to_vaddr(QEMU_INITRAMFS_START) as *const u8,
                QEMU_INITRAMFS_END - QEMU_INITRAMFS_START,
            )
        });
    }
    let (start, end) = parse_initramfs_range()?;
    Some(unsafe { core::slice::from_raw_parts(paddr_to_vaddr(start) as *const u8, end - start) })
}

fn parse_acpi_arg() -> BootloaderAcpiArg {
    // TDDO: Add ACPI support for RISC-V, maybe.
    BootloaderAcpiArg::NotProvided
}

fn parse_framebuffer_info() -> Option<BootloaderFramebufferArg> {
    // TODO: Parse framebuffer info from device tree.
    None
}

fn parse_memory_regions() -> MemoryRegionArray {
    let mut regions = MemoryRegionArray::new();
    // Use the actual DRAM base from the device tree (0x40000000 for QEMU, 0 for RPi3).
    let dram_base = crate::arch::board::dram_base();
    let usable_start = dram_base;
    // 512 MB scan window from the DRAM base — enough for QEMU (512 MB) and RPi3 first GB.
    let usable_end = dram_base.saturating_add(QEMU_VIRT_RAM_SCAN_SIZE);
    let (kernel_phys_start, _) = kernel_phys_range();

    if crate::arch::board::BoardType::cached() != 2 {
        regions
            .push(MemoryRegion::new(
                usable_start,
                usable_end - usable_start,
                MemoryRegionType::Usable,
            ))
            .unwrap();
    } else {
        for region in DEVICE_TREE.get().unwrap().memory().regions() {
            if region.size.unwrap_or(0) > 0 {
                let region_start = region.starting_address as usize;
                let region_end = region_start + region.size.unwrap();
                let clipped_start = region_start.max(usable_start);
                let clipped_end = region_end.min(usable_end);
                if clipped_start >= clipped_end {
                    continue;
                }

                regions
                    .push(MemoryRegion::new(
                        clipped_start,
                        clipped_end - clipped_start,
                        MemoryRegionType::Usable,
                    ))
                    .unwrap();
            }
        }

        if let Some(node) = DEVICE_TREE.get().unwrap().find_node("/reserved-memory") {
            for child in node.children() {
                if let Some(reg_iter) = child.reg() {
                    for region in reg_iter {
                        let region_start = region.starting_address as usize;
                        let region_end = region_start + region.size.unwrap();
                        let clipped_start = region_start.max(usable_start);
                        let clipped_end = region_end.min(usable_end);
                        if clipped_start >= clipped_end {
                            continue;
                        }

                        regions
                            .push(MemoryRegion::new(
                                clipped_start,
                                clipped_end - clipped_start,
                                MemoryRegionType::Reserved,
                            ))
                            .unwrap();
                    }
                }
            }
        }
    }

    if let Some((dtb_base, dtb_size)) = DEVICE_TREE_REGION.get().copied() {
        let aligned_base = dtb_base & !(crate::mm::PAGE_SIZE - 1);
        let aligned_end =
            (dtb_base + dtb_size + crate::mm::PAGE_SIZE - 1) & !(crate::mm::PAGE_SIZE - 1);
        regions
            .push(MemoryRegion::new(
                aligned_base,
                aligned_end - aligned_base,
                MemoryRegionType::Reserved,
            ))
            .unwrap();
    }

    if kernel_phys_start > usable_start {
        regions
            .push(MemoryRegion::new(
                usable_start,
                kernel_phys_start - usable_start,
                MemoryRegionType::Reserved,
            ))
            .unwrap();
    }

    // Add the kernel region.
    regions.push(MemoryRegion::kernel()).unwrap();

    // Add the initramfs region.
    if let Some((start, end)) = parse_initramfs_range() {
        regions
            .push(MemoryRegion::new(
                start,
                end - start,
                MemoryRegionType::Module,
            ))
            .unwrap();
    }

    regions.into_non_overlapping()
}

fn parse_initramfs_range() -> Option<(usize, usize)> {
    if crate::arch::board::BoardType::cached() != 2 {
        return Some((QEMU_INITRAMFS_START, QEMU_INITRAMFS_END));
    }
    let chosen = DEVICE_TREE.get().unwrap().find_node("/chosen")?;
    let initrd_start_prop = chosen.property("linux,initrd-start")?;
    let initrd_start = initrd_start_prop.as_usize()?;
    let initrd_end = chosen.property("linux,initrd-end")?.as_usize()?;
    Some((initrd_start, initrd_end))
}

fn parse_fdt_total_size(dtb_ptr: *const u8) -> usize {
    let total_size_bytes = unsafe { core::slice::from_raw_parts(dtb_ptr.add(4), 4) };
    u32::from_be_bytes(total_size_bytes.try_into().unwrap()) as usize
}

fn kernel_phys_range() -> (usize, usize) {
    unsafe extern "C" {
        fn __kernel_start();
        fn __kernel_end();
    }

    let dram_base = crate::arch::board::dram_base();
    let offset = crate::mm::kspace::kernel_loaded_offset();
    let start = dram_base + (__kernel_start as usize - offset);
    let end = dram_base + (__kernel_end as usize - offset);
    (start, end)
}

fn is_valid_dtb_paddr(paddr: usize, scan_end: usize) -> bool {
    if paddr + 8 > scan_end {
        return false;
    }

    let dtb_ptr = paddr as *const u8;
    let magic = unsafe { core::slice::from_raw_parts(dtb_ptr, 4) };
    if magic != FDT_MAGIC_BE {
        return false;
    }

    let total_size = parse_fdt_total_size(dtb_ptr);
    if !(0x100..=FDT_MAX_TOTAL_SIZE).contains(&total_size) {
        return false;
    }
    if paddr + total_size > scan_end {
        return false;
    }

    // Magic bytes and size bounds are sufficient for early-boot validation.
    // Do NOT call Fdt::from_ptr here: it applies strict version checks that
    // reject valid RPi3 DTBs (FDT v16 compatibility level), causing
    // discover_dtb_paddr() to return None and the kernel to hang silently.
    true
}

fn discover_dtb_paddr(device_tree_paddr: usize) -> Option<usize> {
    // Use a fixed upper limit large enough to cover:
    //   - RPi3 DTB: typically at PA 0x02000000–0x04000000
    //   - QEMU loader DTB: at PA 0x47000000
    //   - QEMU virt RAM: 0x40000000 + 512 MB = 0x60000000
    const DTB_SCAN_END: usize = 0x6000_0000;

    if device_tree_paddr != 0 && is_valid_dtb_paddr(device_tree_paddr, DTB_SCAN_END) {
        return Some(device_tree_paddr);
    }

    if is_valid_dtb_paddr(QEMU_LOADER_DTB_PADDR, DTB_SCAN_END) {
        return Some(QEMU_LOADER_DTB_PADDR);
    }

    None
}

/// The entry point of the Rust code portion of Asterinas.
///
/// AArch64 Linux boot protocol: x0 = physical address of DTB, x1 = 0 (reserved).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aarch64_boot(device_tree_paddr: usize, _reserved: usize) -> ! {
    use crate::boot::{call_ostd_main, EarlyBootInfo, EARLY_INFO};

    // Install our own exception vectors ASAP so any EL1 fault goes to our
    // handler (prints ESR/ELR/SPSR and loops) instead of U-Boot's which resets.
    unsafe { crate::arch::trap::init() };

    // CRITICAL: detect the board type from the raw DTB pointer BEFORE any UART output.
    // The runtime console uses BoardType::cached(), so we must populate the cache first.
    // discover_dtb_paddr() validates the DTB via the TTBR0 identity map (no MMU tricks needed).
    let discovered_dtb_paddr = discover_dtb_paddr(device_tree_paddr).unwrap_or(0);

    crate::arch::board::BoardType::detect_from_dtb_ptr(discovered_dtb_paddr);


    if discovered_dtb_paddr != 0 {
        let device_tree_ptr = discovered_dtb_paddr as *const u8;
        let device_tree_size = parse_fdt_total_size(device_tree_ptr);

        // Copy the DTB into a high-half, page-table-backed buffer before parsing.
        // The fdt crate works with Rust slices; this guarantees the data is in a
        // stable, lifetime-compatible location and is not confused with a physical
        // address that may later be unmapped.
        let dtb_copy_ptr = core::ptr::addr_of_mut!(DTB_COPY).cast::<u8>();
        unsafe {
            core::ptr::copy_nonoverlapping(device_tree_ptr, dtb_copy_ptr, device_tree_size);
        }
        let dtb_slice: &'static [u8] = unsafe {
            core::slice::from_raw_parts(core::ptr::addr_of!(DTB_COPY).cast::<u8>(), device_tree_size)
        };

        if crate::arch::board::BoardType::cached() == 2 {
            let fdt = match fdt::Fdt::new(dtb_slice) {
                Ok(f) => f,
                Err(_e) => {
                    loop { core::hint::spin_loop(); }
                }
            };
            DEVICE_TREE.call_once(|| fdt);
        }
        DEVICE_TREE_REGION.call_once(|| (discovered_dtb_paddr, device_tree_size));
        let initramfs_info = parse_initramfs();
        if initramfs_info.is_some() {
        } else {
        }
        EARLY_INFO.call_once(|| EarlyBootInfo {
            bootloader_name: parse_bootloader_name(),
            kernel_cmdline: parse_kernel_commandline(),
            initramfs: initramfs_info,
            acpi_arg: parse_acpi_arg(),
            framebuffer_arg: parse_framebuffer_info(),
            memory_regions: parse_memory_regions(),
        });
    } else {
        loop {
            core::hint::spin_loop();
        }
    }

    call_ostd_main();
}
