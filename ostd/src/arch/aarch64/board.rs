// SPDX-License-Identifier: MPL-2.0

//! Board detection for AArch64 platforms.
//!
//! Detects whether we're running on QEMU virt or Raspberry Pi 3B/3B+ hardware.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

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
    /// Parses the FDT's root `/compatible` property to identify RPi3.
    /// Returns and caches QemuVirt if the pointer is null, the FDT is invalid,
    /// or no RPi3 compatible string is found.
    pub fn detect_from_dtb_ptr(dtb_ptr: usize) -> Self {
        if dtb_ptr == 0 {
            Self::cache(BoardType::QemuVirt);
            return BoardType::QemuVirt;
        }

        // Verify the FDT magic bytes (big-endian 0xd0_0d_fe_ed).
        let header = unsafe { core::slice::from_raw_parts(dtb_ptr as *const u8, 4) };
        if header != [0xd0, 0x0d, 0xfe, 0xed] {
            Self::cache(BoardType::QemuVirt);
            return BoardType::QemuVirt;
        }

        // Read the FDT total size (big-endian u32 at offset 4).
        let total_size_bytes =
            unsafe { core::slice::from_raw_parts((dtb_ptr + 4) as *const u8, 4) };
        let total_size =
            u32::from_be_bytes([total_size_bytes[0], total_size_bytes[1], total_size_bytes[2], total_size_bytes[3]])
                as usize;
        // Clamp to a reasonable maximum (2 MB) to avoid huge scans.
        let scan_size = total_size.min(2 * 1024 * 1024);

        // Scan raw DTB bytes for well-known RPi3 compatible strings.
        // This avoids the `fdt` crate's path-based find_node() which may fail
        // when the root node compatible property is not directly accessible via
        // find_node("/") (some FDT implementations or crate versions differ).
        let dtb_bytes = unsafe { core::slice::from_raw_parts(dtb_ptr as *const u8, scan_size) };
        let board = if dtb_bytes.windows(7).any(|w| w == b"bcm2837")
            || dtb_bytes.windows(11).any(|w| w == b"raspberrypi")
        {
            BoardType::RaspberryPi3
        } else {
            BoardType::QemuVirt
        };

        if board.is_hardware() {
            IS_HARDWARE.store(true, Ordering::Relaxed);
        }
        Self::cache(board);
        board
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
static DRAM_BASE_CACHE: AtomicUsize = AtomicUsize::new(usize::MAX);

pub fn dram_base() -> usize {
    use crate::arch::boot::DEVICE_TREE;

    let cached = DRAM_BASE_CACHE.load(Ordering::Relaxed);
    if cached != usize::MAX {
        return cached;
    }

    let base = if let Some(fdt) = DEVICE_TREE.get() {
        if let Some(region) = fdt.memory().regions().next() {
            region.starting_address as usize
        } else {
            0x4000_0000
        }
    } else {
        0x4000_0000
    };

    DRAM_BASE_CACHE.store(base, Ordering::Relaxed);
    base
}
