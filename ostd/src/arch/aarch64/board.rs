// SPDX-License-Identifier: MPL-2.0

//! Board detection for AArch64 platforms.
//!
//! Detects whether we're running on QEMU virt or Raspberry Pi 3B/3B+ hardware.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Set to true once board detection runs and identifies real hardware.
pub(crate) static IS_HARDWARE: AtomicBool = AtomicBool::new(false);

/// Cached board type: 0 = unknown, 1 = QemuVirt, 2 = RaspberryPi3.
/// Used by early_puts before DEVICE_TREE is available.
static BOARD_CACHE: AtomicU8 = AtomicU8::new(0);

/// Magic bytes at offset 4 in a valid FDT blob (big-endian).
const FDT_MAGIC: u32 = 0xedfe0dd0;

/// Board types supported by Asterinas on AArch64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardType {
    /// QEMU virtual machine (virt board).
    QemuVirt,
    /// Raspberry Pi 3 Model B or B+.
    RaspberryPi3,
}

impl BoardType {
    /// Returns the detected board type.
    ///
    /// After the first detection (which caches the result), subsequent calls
    /// return the cached value WITHOUT touching the FDT/DTB. This is critical:
    /// after the KPT switch the `DEVICE_TREE` object stores a raw physical-address
    /// pointer that is no longer valid as a virtual address, so re-parsing it
    /// would trigger a data-abort.  `detect_from_dtb_ptr()` is always called
    /// before `DEVICE_TREE` is populated, so the cache is always warm by the
    /// time subsequent callers (timer callbacks, IRQ handlers, …) call here.
    pub fn detect() -> Self {
        let cached_val = Self::cached();
        if cached_val != 0 {
            let board = if cached_val == 2 {
                BoardType::RaspberryPi3
            } else {
                BoardType::QemuVirt
            };
            // Keep IS_HARDWARE in sync even on the fast path.
            if board.is_hardware() {
                IS_HARDWARE.store(true, Ordering::Relaxed);
            }
            return board;
        }
        // First call: cache not populated yet – parse from FDT.
        let board = Self::detect_from_fdt();
        if board.is_hardware() {
            IS_HARDWARE.store(true, Ordering::Relaxed);
        }
        Self::cache(board);
        board
    }

    /// Detects board type from raw DTB pointer (before DEVICE_TREE is set).
    /// Used by early_puts before any Rust infrastructure is available.
    /// Returns QemuVirt if DTB pointer is null or invalid.
    pub fn detect_from_dtb_ptr(dtb_ptr: usize) -> Self {
        // Check the boot code's board detection first (set in boot.S based on PC).
        // Marker is placed in .text so Rust can access it; boot.S writes via phys addr.
        #[unsafe(no_mangle)]
        #[unsafe(link_section = ".text")]
        static mut BOOT_BOARD_IS_RPI3: u64 = 0;
        if unsafe { BOOT_BOARD_IS_RPI3 } != 0 {
            Self::cache(BoardType::RaspberryPi3);
            return BoardType::RaspberryPi3;
        }

        // BOOT_BOARD_IS_RPI3 = 0 means QEMU virt (PC >= 0x4000_0000).
        // QEMU also passes a valid DTB in x0, so we cannot use dtb_ptr validity
        // to distinguish QEMU from RPi3 — trust the PC-based marker instead.
        Self::cache(BoardType::QemuVirt);
        BoardType::QemuVirt
    }

    /// Returns true if running on QEMU (needs TLB workarounds).
    pub fn is_qemu(&self) -> bool {
        matches!(self, BoardType::QemuVirt)
    }

    /// Returns true if running on real hardware (safe for full TLBI).
    pub fn is_hardware(&self) -> bool {
        !self.is_qemu()
    }

    /// Cache the board type into a global atomic for early access.
    fn cache(board: BoardType) {
        let val = match board {
            BoardType::QemuVirt => 1,
            BoardType::RaspberryPi3 => 2,
        };
        BOARD_CACHE.store(val, Ordering::Relaxed);
    }

    /// Returns the cached board type, or 0 if not yet detected.
    pub fn cached() -> u8 {
        BOARD_CACHE.load(Ordering::Relaxed)
    }

    fn detect_from_fdt() -> Self {
        use crate::arch::boot::DEVICE_TREE;

        let Some(fdt) = DEVICE_TREE.get() else {
            return BoardType::QemuVirt;
        };

        let Some(root_compatible) = fdt.find_node("/").and_then(|n| n.property("compatible"))
        else {
            return BoardType::QemuVirt;
        };

        if let Some(compat_str) = root_compatible.as_str() {
            if compat_str.contains("bcm2837") || compat_str.contains("raspi") {
                return BoardType::RaspberryPi3;
            }
        }

        let compat_bytes = root_compatible.value;
        if compat_bytes
            .windows(7)
            .any(|w| w == b"bcm2837" || w == b"raspi3")
        {
            return BoardType::RaspberryPi3;
        }

        BoardType::QemuVirt
    }
}

/// Returns the base physical address of DRAM from the device tree.
///
/// Reads the `/memory` node `reg` property to determine where RAM starts.
/// Defaults to `0x4000_0000` (QEMU virt) if the DTB is unavailable.
pub fn dram_base() -> usize {
    use crate::arch::boot::DEVICE_TREE;

    let Some(fdt) = DEVICE_TREE.get() else {
        return 0x4000_0000;
    };

    let Some(region) = fdt.memory().regions().next() else {
        return 0x4000_0000;
    };

    region.starting_address as usize
}
