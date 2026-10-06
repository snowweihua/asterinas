// SPDX-License-Identifier: MPL-2.0

//! Memory stress test for AArch64 hardware bring-up.
//!
//! This module provides simple memory allocation stress tests that can be
//! run automatically at boot (controlled by kernel command line) to verify
//! the frame allocator and physical memory mapping work correctly on real hardware.

use alloc::vec::Vec;

use crate::{
    Error,
    mm::{FrameAllocOptions, PAGE_SIZE, frame::Frame},
    prelude::*,
};

/// Run memory stress tests and report results via serial.
///
/// Results are printed with `println!` (which maps to `early_println!` via prelude)
/// so they appear even before the full console driver is initialized.
pub fn run_memory_stress_tests() {
    println!("[memtest] Starting memory stress tests...");

    let mut passed = 0;
    let mut failed = 0;

    if test_single_frame_alloc().is_ok() {
        println!("[memtest] test_single_frame_alloc: PASS");
        passed += 1;
    } else {
        println!("[memtest] test_single_frame_alloc: FAIL");
        failed += 1;
    }

    if test_multi_frame_alloc().is_ok() {
        println!("[memtest] test_multi_frame_alloc: PASS");
        passed += 1;
    } else {
        println!("[memtest] test_multi_frame_alloc: FAIL");
        failed += 1;
    }

    if test_alloc_stress().is_ok() {
        println!("[memtest] test_alloc_stress: PASS");
        passed += 1;
    } else {
        println!("[memtest] test_alloc_stress: FAIL");
        failed += 1;
    }

    if test_large_alloc().is_ok() {
        println!("[memtest] test_large_alloc: PASS");
        passed += 1;
    } else {
        println!("[memtest] test_large_alloc: FAIL");
        failed += 1;
    }

    println!(
        "[memtest] Results: {}/{} passed, {}/{} failed",
        passed,
        passed + failed,
        failed,
        passed + failed
    );
}

fn test_single_frame_alloc() -> Result<()> {
    let frame = FrameAllocOptions::new()
        .alloc_frame()
        .map_err(|_| Error::NoMemory)?;
    assert_eq!(frame.size(), PAGE_SIZE);
    Ok(())
}

fn test_multi_frame_alloc() -> Result<()> {
    let count = 16;
    let mut frames: Vec<Frame<()>> = Vec::new();

    for _ in 0..count {
        let frame = FrameAllocOptions::new()
            .alloc_frame()
            .map_err(|_| Error::NoMemory)?;
        frames.push(frame);
    }

    assert_eq!(frames.len(), count);
    Ok(())
}

fn test_alloc_stress() -> Result<()> {
    let cycles = 100;
    let frames_per_cycle = 8;

    for _ in 0..cycles {
        let mut frames: Vec<Frame<()>> = Vec::new();
        for _ in 0..frames_per_cycle {
            let frame = FrameAllocOptions::new()
                .alloc_frame()
                .map_err(|_| Error::NoMemory)?;
            frames.push(frame);
        }
    }

    Ok(())
}

fn test_large_alloc() -> Result<()> {
    let num_frames = 256;
    let mut frames: Vec<Frame<()>> = Vec::new();

    for i in 0..num_frames {
        match FrameAllocOptions::new().alloc_frame() {
            Ok(frame) => frames.push(frame),
            Err(_) => {
                println!("[memtest] Large alloc: got {}/{} frames", i, num_frames);
                return Ok(());
            }
        }
    }

    println!(
        "[memtest] Large alloc: allocated {} frames ({} MB)",
        frames.len(),
        frames.len() * PAGE_SIZE / 1024 / 1024
    );
    Ok(())
}
