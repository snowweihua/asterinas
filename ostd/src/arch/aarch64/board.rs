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

        // Parse the FDT to read the root /compatible string.
        // SAFETY: we have verified the magic bytes and the DTB is accessible via
        // the boot identity map (TTBR0) which covers the full 32-bit PA space.
        let board = match unsafe { fdt::Fdt::from_ptr(dtb_ptr as *const u8) } {
            Ok(fdt) => {
                let compat_bytes = fdt
                    .find_node("/")
                    .and_then(|n| n.property("compatible"))
                    .map(|p| p.value)
                    .unwrap_or(&[]);
                // Match BCM2837 (RPi3 3B/3B+) or generic "raspberrypi" compatible strings.
                if compat_bytes.windows(7).any(|w| w == b"bcm2837")
                    || compat_bytes.windows(11).any(|w| w == b"raspberrypi")
                {
                    BoardType::RaspberryPi3
                } else {
                    BoardType::QemuVirt
                }
            }
            Err(_) => BoardType::QemuVirt,
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
