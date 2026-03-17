// SPDX-License-Identifier: MPL-2.0

//! The RISC-V boot module defines the entrypoints of Asterinas.

pub mod smp;

use core::arch::global_asm;

use fdt::Fdt;
use spin::Once;

use crate::{
    boot::{
        memory_region::{MemoryRegion, MemoryRegionArray, MemoryRegionType},
        BootloaderAcpiArg, BootloaderFramebufferArg,
    },
    mm::paddr_to_vaddr,
};

global_asm!(include_str!("boot.S"));

// Pure-assembly PL011 helpers that live entirely outside Rust's debug
// machinery.  No volatile-wrapper calls, no ptr::add precondition checks,
// no panic paths — just plain AArch64 instructions.
//
// pl011_puts_asm(ptr: *const u8, len: usize)
//   x0 = pointer to first byte
//   x1 = byte count
//   Clobbers x2-x5; preserves everything else (including lr via ret).
global_asm!(r#"
    .text
    .align 2
    .globl pl011_puts_asm
pl011_puts_asm:
    cbz     x1, 2f
    mov     x2, #0x09000000
1:
    ldrb    w3, [x0], #1
3:
    ldr     w4, [x2, #0x18]
    tbnz    w4, #5, 3b
    str     w3, [x2]
    subs    x1, x1, #1
    bne     1b
2:
    ret
"#);


/// The Flattened Device Tree of the platform.
pub static DEVICE_TREE: Once<Fdt> = Once::new();
static DEVICE_TREE_REGION: Once<(usize, usize)> = Once::new();

const QEMU_VIRT_RAM_BASE: usize = 0x4000_0000;
const QEMU_VIRT_RAM_SCAN_SIZE: usize = 512 * 1024 * 1024;
const FDT_MAGIC_BE: [u8; 4] = [0xd0, 0x0d, 0xfe, 0xed];
const EMBEDDED_QEMU_VIRT_DTB: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../test/nix/aarch64-virt.dtb"));

fn parse_bootloader_name() -> &'static str {
    "Unknown"
}

fn parse_kernel_commandline() -> &'static str {
    DEVICE_TREE.get().unwrap().chosen().bootargs().unwrap_or("")
}

fn parse_initramfs() -> Option<&'static [u8]> {
    let (start, end) = parse_initramfs_range()?;

    let base_va = paddr_to_vaddr(start);
    let length = end - start;
    Some(unsafe { core::slice::from_raw_parts(base_va as *const u8, length) })
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

    for region in DEVICE_TREE.get().unwrap().memory().regions() {
        if region.size.unwrap_or(0) > 0 {
            regions
                .push(MemoryRegion::new(
                    region.starting_address as usize,
                    region.size.unwrap(),
                    MemoryRegionType::Usable,
                ))
                .unwrap();
        }
    }

    if let Some(node) = DEVICE_TREE.get().unwrap().find_node("/reserved-memory") {
        for child in node.children() {
            if let Some(reg_iter) = child.reg() {
                for region in reg_iter {
                    regions
                        .push(MemoryRegion::new(
                            region.starting_address as usize,
                            region.size.unwrap(),
                            MemoryRegionType::Reserved,
                        ))
                        .unwrap();
                }
            }
        }
    }

    if let Some((dtb_base, dtb_size)) = DEVICE_TREE_REGION.get().copied() {
        let aligned_base = dtb_base & !(crate::mm::PAGE_SIZE - 1);
        let aligned_end = (dtb_base + dtb_size + crate::mm::PAGE_SIZE - 1)
            & !(crate::mm::PAGE_SIZE - 1);
        regions
            .push(MemoryRegion::new(
                aligned_base,
                aligned_end - aligned_base,
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
    let chosen = DEVICE_TREE.get().unwrap().find_node("/chosen").unwrap();
    let initrd_start = chosen.property("linux,initrd-start")?.as_usize()?;
    let initrd_end = chosen.property("linux,initrd-end")?.as_usize()?;
    Some((initrd_start, initrd_end))
}

fn parse_fdt_total_size(dtb_ptr: *const u8) -> usize {
    let total_size_bytes = unsafe { core::slice::from_raw_parts(dtb_ptr.add(4), 4) };
    u32::from_be_bytes(total_size_bytes.try_into().unwrap()) as usize
}

fn find_dtb_paddr_in_qemu_ram() -> Option<usize> {
    let scan_start = QEMU_VIRT_RAM_BASE;
    let scan_end = QEMU_VIRT_RAM_BASE + QEMU_VIRT_RAM_SCAN_SIZE;

    let is_valid_fdt = |paddr: usize| {
        let dtb_ptr = paddr as *const u8;
        let magic = unsafe { core::slice::from_raw_parts(dtb_ptr, 4) };
        magic == FDT_MAGIC_BE && unsafe { Fdt::from_ptr(dtb_ptr) }.is_ok()
    };

    let dense_scan = |start: usize, end: usize| {
        let mut paddr = start;
        while paddr + 4 <= end {
            if is_valid_fdt(paddr) {
                return Some(paddr);
            }
            paddr += 8;
        }
        None
    };

    for paddr in (scan_start..scan_end).step_by(0x1000) {
        if is_valid_fdt(paddr) {
            return Some(paddr);
        }
    }

    let dense_window = 32 * 1024 * 1024;
    let low_window_end = scan_start + dense_window;
    if let Some(found) = dense_scan(scan_start, low_window_end) {
        return Some(found);
    }

    let high_window_start = scan_end - dense_window;
    if let Some(found) = dense_scan(high_window_start, scan_end) {
        return Some(found);
    }

    None
}

fn parse_embedded_qemu_dtb() -> Option<Fdt<'static>> {
    unsafe { Fdt::from_ptr(EMBEDDED_QEMU_VIRT_DTB.as_ptr()) }.ok()
}

/// Declared here; defined in the global_asm! block above.
unsafe extern "C" {
    fn pl011_puts_asm(ptr: *const u8, len: usize);
}

/// Write a byte slice to the PL011 UART.  Safe to call before any Rust
/// runtime infrastructure (no panics, no volatile wrappers, no alloc).
#[inline(always)]
pub unsafe fn pl011_puts(s: &[u8]) {
    unsafe { pl011_puts_asm(s.as_ptr(), s.len()) };
}

#[inline(always)]
pub fn pl011_puts_static(s: &'static [u8]) {
    unsafe { pl011_puts_asm(s.as_ptr(), s.len()) };
}

#[inline(always)]
pub fn pl011_putc(c: u8) {
    let buf = [c];
    unsafe { pl011_puts_asm(buf.as_ptr(), 1) };
}

/// The entry point of the Rust code portion of Asterinas.
///
/// AArch64 Linux boot protocol: x0 = physical address of DTB, x1 = 0 (reserved).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aarch64_boot(device_tree_paddr: usize, _reserved: usize) -> ! {
    // Direct PL011 writes to survive before SpinLock/CPU-local are ready.
    unsafe { pl011_puts(b"[1] aarch64_boot entered\n") };
    unsafe { pl011_puts_asm(b"[2] direct\n".as_ptr(), 11) };
    // early_println!("Enter aarch64_boot");
    unsafe { pl011_puts(b"[3] after early_println\n") };
    // early_println!("  device_tree_paddr = {:#x}", device_tree_paddr);

    use crate::boot::{call_ostd_main, EarlyBootInfo, EARLY_INFO};

    let discovered_dtb_paddr = if device_tree_paddr != 0 {
        device_tree_paddr
    } else {
        find_dtb_paddr_in_qemu_ram().unwrap_or(0)
    };

    unsafe { pl011_puts(b"[5g] before early_info once\n") };
    if discovered_dtb_paddr != 0 {
        if device_tree_paddr == 0 {
            unsafe { pl011_puts(b"[3x] DTB discovered by RAM scan\n") };
        }
        unsafe { pl011_puts(b"[3x] before fdt::from_ptr\n") };
        let device_tree_ptr = discovered_dtb_paddr as *const u8;
        let device_tree_size = parse_fdt_total_size(device_tree_ptr);
        let fdt = unsafe { fdt::Fdt::from_ptr(device_tree_ptr).unwrap() };
        DEVICE_TREE.call_once(|| fdt);
        DEVICE_TREE_REGION.call_once(|| (discovered_dtb_paddr, device_tree_size));
        unsafe { pl011_puts(b"[4] after device_tree once\n") };

        EARLY_INFO.call_once(|| EarlyBootInfo {
            bootloader_name: parse_bootloader_name(),
            kernel_cmdline: parse_kernel_commandline(),
            initramfs: parse_initramfs(),
            acpi_arg: parse_acpi_arg(),
            framebuffer_arg: parse_framebuffer_info(),
            memory_regions: parse_memory_regions(),
        });
    } else if let Some(fdt) = parse_embedded_qemu_dtb() {
        unsafe { pl011_puts(b"[3x] no DTB register; using embedded DTB blob\n") };
        DEVICE_TREE.call_once(|| fdt);

        EARLY_INFO.call_once(|| EarlyBootInfo {
            bootloader_name: parse_bootloader_name(),
            kernel_cmdline: parse_kernel_commandline(),
            initramfs: parse_initramfs(),
            acpi_arg: parse_acpi_arg(),
            framebuffer_arg: parse_framebuffer_info(),
            memory_regions: parse_memory_regions(),
        });
    } else {
        unsafe { pl011_puts(b"[3x] FATAL: no DTB source available\n") };
        loop {
            core::hint::spin_loop();
        }
    }
    unsafe { pl011_puts(b"[5h] after early_info once\n") };

    call_ostd_main();
}
