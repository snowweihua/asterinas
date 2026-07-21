// SPDX-License-Identifier: MPL-2.0

//! The RISC-V boot module defines the entrypoints of Asterinas.

pub mod smp;

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
global_asm!(include_str!("ap_boot.S"));

// Pure-assembly PL011 helpers that live entirely outside Rust's debug
// machinery.  No volatile-wrapper calls, no ptr::add precondition checks,
// no panic paths — just plain AArch64 instructions.
//
// pl011_puts_asm(ptr: *const u8, len: usize, uart_base: usize)
//   x0 = pointer to first byte
//   x1 = byte count
//   x2 = UART base PA
//   Clobbers x3-x5; preserves everything else (including lr via ret).
global_asm!(
    r#"
    .text
    .align 2
    .globl pl011_puts_asm
pl011_puts_asm:
    cbz     x1, 2f
1:
    ldrb    w3, [x0], #1
    str     w3, [x2]
    subs    x1, x1, #1
    bne     1b
2:
    ret
"#
);

/// The Flattened Device Tree of the platform.
pub static DEVICE_TREE: Once<Fdt> = Once::new();
static DEVICE_TREE_REGION: Once<(usize, usize)> = Once::new();

const QEMU_VIRT_RAM_BASE: usize = 0x4000_0000;
const QEMU_VIRT_RAM_SCAN_SIZE: usize = 512 * 1024 * 1024;
const QEMU_LOADER_DTB_PADDR: usize = 0x4700_0000;
const FDT_MAX_TOTAL_SIZE: usize = 2 * 1024 * 1024;
const FDT_MAGIC_BE: [u8; 4] = [0xd0, 0x0d, 0xfe, 0xed];

pub fn kernel_physical_base(kernel_start: usize, kernel_loaded_offset: usize) -> usize {
    crate::arch::board::dram_base() + (kernel_start - kernel_loaded_offset)
}

fn parse_bootloader_name() -> &'static str {
    "Unknown"
}

fn parse_kernel_commandline() -> &'static str {
    DEVICE_TREE.get().unwrap().chosen().bootargs().unwrap_or("")
}

fn parse_initramfs() -> Option<&'static [u8]> {
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
    let chosen = DEVICE_TREE.get().unwrap().find_node("/chosen").unwrap();
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

// Declared here; defined in the global_asm! block above.
unsafe extern "C" {
    fn pl011_puts_asm(ptr: *const u8, len: usize, uart_base: usize);
}

fn early_uart_base() -> usize {
    if crate::arch::board::BoardType::cached() == 2 {
        0x3F20_1000
    } else {
        0x0900_0000
    }
}

pub fn pl011_puts_safe(s: &[u8]) {
    unsafe { pl011_puts(s) }
}

#[inline(never)]
pub unsafe fn pl011_puts(s: &[u8]) {
    let mut i = 0;
    while i < s.len() {
        let b = s[i];
        unsafe {
            core::arch::asm!(
                "movz x4, #0x3F21, lsl #16",
                "movk x4, #0x5054",
                "1: ldrb w5, [x4]",
                "tst w5, #0x20",
                "beq 1b",
                out("x4") _,
                out("w5") _,
                options(nostack),
            );
        }
        unsafe {
            core::arch::asm!(
                "movz x4, #0x3F21, lsl #16",
                "movk x4, #0x5040",
                "strb w3, [x4]",
                in("w3") b as u32,
                out("x4") _,
                options(nostack),
            );
        }
        i += 1;
    }
}

#[inline(always)]
fn early_marker(ch: u8) {
    unsafe {
        core::arch::asm!(
            "movz x28, #0x3F21, lsl #16",
            "movk x28, #0x5040",
            "strb w27, [x28]",
            in("w27") ch as u32,
            out("x28") _,
            options(nostack),
        );
    }
}

/// Print a hex value to the mini-UART. Used for diagnostic markers.
/// Pure-register implementation: no stack frame, no local allocations.
/// Uses the same UART address as pl011_puts (RPi3 mini-UART DR at 0x3F215040).
#[inline(never)]
pub unsafe fn pl011_puts_hex(val: usize) {
    // Write "0x" prefix
    for &byte in b"0x" {
        // Busy-wait for TX FIFO to have space (LSR bit 5)
        unsafe { core::arch::asm!("movz x4, #0x3F21, lsl #16", "movk x4, #0x5054", "1: ldrb w5, [x4]", "tst w5, #0x20", "beq 1b", out("x4") _, out("w5") _, options(nostack)); }
        unsafe { core::arch::asm!("movz x4, #0x3F21, lsl #16", "movk x4, #0x5040", "strb w3, [x4]", in("w3") byte as u32, out("x4") _, options(nostack)); }
    }
    // Write 16 hex nibbles
    for nibble_pos in 0..16 {
        let shift = (15 - nibble_pos) * 4;
        let nibble = (val >> shift) & 0xf;
        let c = if nibble < 10 { b'0' + nibble as u8 } else { b'a' + nibble as u8 - 10 };
        unsafe { core::arch::asm!("movz x4, #0x3F21, lsl #16", "movk x4, #0x5054", "1: ldrb w5, [x4]", "tst w5, #0x20", "beq 1b", out("x4") _, out("w5") _, options(nostack)); }
        unsafe { core::arch::asm!("movz x4, #0x3F21, lsl #16", "movk x4, #0x5040", "strb w3, [x4]", in("w3") c as u32, out("x4") _, options(nostack)); }
    }
    // newline
    unsafe { core::arch::asm!("movz x4, #0x3F21, lsl #16", "movk x4, #0x5054", "1: ldrb w5, [x4]", "tst w5, #0x20", "beq 1b", out("x4") _, out("w5") _, options(nostack)); }
    unsafe { core::arch::asm!("movz x4, #0x3F21, lsl #16", "movk x4, #0x5040", "mov w5, #10", "strb w5, [x4]", out("x4") _, out("w5") _, options(nostack)); }
}

/// The entry point of the Rust code portion of Asterinas.
///
/// AArch64 Linux boot protocol: x0 = physical address of DTB, x1 = 0 (reserved).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aarch64_boot(device_tree_paddr: usize, _reserved: usize) -> ! {
    use crate::boot::{call_ostd_main, EarlyBootInfo, EARLY_INFO};

    // DEBUG MARKER 'F': first instruction reached in Rust.
    // Writes 'F' unconditionally to RPi3 PL011, RPi3 mini-UART, and QEMU PL011
    // so we see it regardless of board type, before board detection.
    unsafe {
        core::arch::asm!(
            // PL011 DR on RPi3: 0x3F201000
            "movz x28, #0x3F20, lsl #16",
            "movk x28, #0x1000",
            "mov  w27, #70",           // 'F'
            "str  w27, [x28]",
            // mini-UART IO on RPi3: 0x3F215040
            "movz x28, #0x3F21, lsl #16",
            "movk x28, #0x5040",
            "str  w27, [x28]",
            // PL011 DR on QEMU virt: 0x09000000
            "movz x28, #0x0900, lsl #16",
            "str  w27, [x28]",
            out("x27") _,
            out("x28") _,
            options(nostack),
        );
    }

    // Install our own exception vectors ASAP so any EL1 fault goes to our
    // handler (prints ESR/ELR/SPSR and loops) instead of U-Boot's which resets.
    unsafe { crate::arch::trap::init() };

    // MARKER G: trap::init() done.
    unsafe { pl011_puts(b"G\n"); }

    // CRITICAL: detect the board type from the raw DTB pointer BEFORE any UART output.
    // early_uart_base() uses BoardType::cached(), so we must populate the cache first.
    // On RPi3, the PL011 is at 0x3F201000; on QEMU it is at 0x09000000.
    // discover_dtb_paddr() validates the DTB via the TTBR0 identity map (no MMU tricks needed).
    let discovered_dtb_paddr = discover_dtb_paddr(device_tree_paddr).unwrap_or(0);

    // MARKER H: discover_dtb_paddr() done.
    unsafe { pl011_puts(b"H\n"); }

    crate::arch::board::BoardType::detect_from_dtb_ptr(discovered_dtb_paddr);

    // MARKER I: detect_from_dtb_ptr() done — board type is now in BOARD_CACHE.
    unsafe { pl011_puts(b"I\n"); }

    unsafe { pl011_puts(b"[a2-boot] entry\n") };

    if discovered_dtb_paddr != 0 {
        unsafe { pl011_puts(b"[a2-boot] using loader dtb\n") };
        let device_tree_ptr = discovered_dtb_paddr as *const u8;
        let device_tree_size = parse_fdt_total_size(device_tree_ptr);
        // Use from_ptr with a fallback: RPi3 DTBs (FDT v16) may be rejected
        // by strict version checks in the fdt crate.  If parsing fails, halt
        // with an explicit message rather than silently panicking.
        let fdt = match unsafe { fdt::Fdt::from_ptr(device_tree_ptr) } {
            Ok(f) => f,
            Err(_e) => {
                unsafe {
                    pl011_puts(b"[a2-boot] FATAL: Fdt::from_ptr failed\n");
                }
                loop { core::hint::spin_loop(); }
            }
        };
        DEVICE_TREE.call_once(|| fdt);
        DEVICE_TREE_REGION.call_once(|| (discovered_dtb_paddr, device_tree_size));
        unsafe { pl011_puts(b"[a2-boot] dtb discovery done\n") };
        let initramfs_info = parse_initramfs();
        if initramfs_info.is_some() {
            unsafe { pl011_puts(b"[a2-boot] initramfs found\n") };
        } else {
            unsafe { pl011_puts(b"[a2-boot] initramfs NOT found\n") };
        }
        unsafe { pl011_puts(b"[a2-boot] cmdline: ") };
        unsafe { pl011_puts(parse_kernel_commandline().as_bytes()) };
        unsafe { pl011_puts(b"\n") };
        EARLY_INFO.call_once(|| EarlyBootInfo {
            bootloader_name: parse_bootloader_name(),
            kernel_cmdline: parse_kernel_commandline(),
            initramfs: initramfs_info,
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

    unsafe { pl011_puts(b"[a2-boot] calling ostd_main\n") };
    call_ostd_main();
}
