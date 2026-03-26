# Current Checkpoint

**Date:** 2026-03-26

## Status
- Build: PASSING (requires `OSDK_LOCAL_DEV=1`)
- Run: **WORKING** - Boot banner, shell prompt, and user space console output all working

## Key Achievement: Console Output Fixed

User space shell (`~ #`) now appears in serial log. The fix was in `ostd/src/arch/aarch64/serial.rs`.

### Root Cause
The `serial::send()` function was using physical address `0x0900_0000` directly, but after MMU is enabled, the kernel must use virtual addresses. The kernel linear map maps PA `0x0900_0000` to VA `0xffff_8000_0900_0000`.

### Fix Applied
Changed `serial.rs` to use the kernel linear map VA instead of the raw PA:
```rust
// Before: const PL011_BASE: usize = 0x0900_0000;
// After: const PL011_BASE_VA: usize = 0xffff_8000_0900_0000;
```

This fix is essential for real hardware (e.g., Raspberry Pi 3B) where physical addresses won't work after kernel starts.

## Build Environment

- Requires: `OSDK_LOCAL_DEV=1`
- Build: `cargo build --target aarch64-unknown-none-softfloat -p aster-nix`
- Run: `cargo osdk run --scheme aarch64 --target-arch aarch64`

## Verified Boot Output
```
AMVRB[a2-boot] ...
[kernel] Spawn init thread
[kernel] unpacking the initramfs.cpio.gz to rootfs ...
[kernel] rootfs is ready
[ASCII art banner]
~ #                              <-- USER SPACE SHELL WORKING
```

## Phase Status (from plan-aarch64Support.prompt.md)

### Phase 0 — Reset + Baseline
- [x] P0.1 Reproducible check
- [x] P0.2 Trap path coherence

### Phase 1 — OSTD Single-Core Bring-Up
- [x] P1.1 Boot banner appears (QEMU) - **NOW WORKING**
- [ ] P1.2 Timer IRQ liveness
- [ ] P1.3 Panic backtrace sanity

### Phase 2 — EL0/EL1 Transition + Trap/Syscall
- [x] P2.1 User transition round-trip - **NOW WORKING** (context switch confirmed)
- [x] P2.2 Syscall smoke - **NOW WORKING** (shell makes syscalls)
- [ ] P2.3 Page fault handoff path

### Phase 3 — Kernel AArch64 Integration
- [x] P3.1 Kernel arch wiring check
- [ ] P3.2 Syscall subset test

### Phase 4 — OSDK Build/Run Pipeline
- [x] P4.1 OSDK build path
- [x] P4.2 OSDK run path

## Files Modified in This Session
- `ostd/src/arch/aarch64/serial.rs` - Use correct VA for PL011 UART
- `ostd/src/arch/aarch64/mod.rs` - Made uart_probe safe
- `ostd/src/task/mod.rs` - Updated uart_probe calls
- `kernel/src/driver/mod.rs` - Added Pl011Console for AArch64
- `kernel/src/device/tty/mod.rs` - Removed duplicate code
- `kernel/src/device/tty/n_tty.rs` - Clean implementation

## Next Steps
1. Test timer interrupts (P1.2)
2. Add panic backtrace test (P1.3)
3. Verify poweroff/exit works cleanly
4. Test page fault handling (P2.3)
