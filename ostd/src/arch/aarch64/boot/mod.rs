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
    early_println,
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

/// Declared here; defined in the global_asm! block above.
unsafe extern "C" {
    fn pl011_puts_asm(ptr: *const u8, len: usize);
}

/// Write a byte slice to the PL011 UART.  Safe to call before any Rust
/// runtime infrastructure (no panics, no volatile wrappers, no alloc).
#[inline(always)]
unsafe fn pl011_puts(s: &[u8]) {
    unsafe { pl011_puts_asm(s.as_ptr(), s.len()) };
}

/// The entry point of the Rust code portion of Asterinas.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aarch64_boot(_hart_id: usize, device_tree_paddr: usize) -> ! {
    // Direct PL011 writes to survive before SpinLock/CPU-local are ready.
    unsafe { pl011_puts(b"[1] aarch64_boot entered\n") };
    unsafe { pl011_puts_asm(b"[2] direct\n".as_ptr(), 11) };
    // early_println!("Enter aarch64_boot");
    unsafe { pl011_puts(b"[3] after early_println\n") };
    // early_println!("  device_tree_paddr = {:#x}", device_tree_paddr);

    let device_tree_ptr = paddr_to_vaddr(device_tree_paddr) as *const u8;
    let fdt = unsafe { fdt::Fdt::from_ptr(device_tree_ptr).unwrap() };
    DEVICE_TREE.call_once(|| fdt);

    use crate::boot::{call_ostd_main, EarlyBootInfo, EARLY_INFO};

    EARLY_INFO.call_once(|| EarlyBootInfo {
        bootloader_name: parse_bootloader_name(),
        kernel_cmdline: parse_kernel_commandline(),
        initramfs: parse_initramfs(),
        acpi_arg: parse_acpi_arg(),
        framebuffer_arg: parse_framebuffer_info(),
        memory_regions: parse_memory_regions(),
    });

    call_ostd_main();
}
