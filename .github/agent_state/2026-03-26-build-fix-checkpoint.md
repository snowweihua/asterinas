# Agent State Checkpoint

Date: 2026-03-26

## Summary
- Build environment: FIXED
  - Restored `/tmp/arm-gic-patched` (was missing)
  - Rebuilt OSDK with `OSDK_LOCAL_DEV=1` flag
- Build: PASSING with `OSDK_LOCAL_DEV=1 cargo build --target aarch64-unknown-none-softfloat -p aster-nix`
- Run: Boots past initramfs unpacking, reaches "rootfs is ready", but stuck in init process loading animation

## Build Environment Fix Details

### Problem 1: Missing arm-gic-patched
- `/tmp/arm-gic-patched` directory was missing
- Required for `is_multiple_of` -> modulo patches for QEMU nightly compatibility
- **Fix**: Rebuilt from arm-gic 0.7.1, applied patches:
  - `is_multiple_of(32)` -> `% 32 == 0`
  - `is_multiple_of(Self::BITS_PER_INTERRUPT)` -> `% Self::BITS_PER_INTERRUPT == 0`
  - etc.

### Problem 2: OSDK using published crates.io versions
- OSDK was pulling `ostd-0.16.1` from crates.io instead of local
- **Fix**: Rebuilt OSDK with `OSDK_LOCAL_DEV=1 cargo install cargo-osdk --path osdk --force`

## Current Blocker

System boots past initramfs unpacking but gets stuck in init process loading animation.

### Observed Behavior
- Boot: `[a2-boot] x0=0, skipping` through `[a2-boot] cmdline: non-empty`
- Kernel: `Spawn init thread`, `unpacking the initramfs.cpio.gz to rootfs ...`, `rootfs is ready`
- Then: Loading animation ("Releasedummy My the Asterinas Linux") repeats forever
- Serial log stays at 35 lines (no new output after "rootfs is ready")
- Timeout after 120s

### Root Cause Hypothesis
The init process (busybox) is starting but stuck waiting for something:
1. Timer interrupt not working (timer interrupts not reaching user space)
2. Serial output buffered but not flushed
3. Init process in some syscall that never returns

### Tried Approaches
1. TLB flush no-ops: BROKE early boot (initramfs unpacking failed)
2. TTBR0 toggle workaround: Works for boot but causes page fault loops later

## Key Files
- `ostd/src/arch/aarch64/mm/mod.rs` — TLB flush functions
- `ostd/src/arch/aarch64/cpu/context.rs` — UserContextApiInternal::execute()
- `kernel/src/arch/aarch64/signal.rs` — AArch64 signal handling
- `kernel/src/lib.rs` — Kernel main entry

## Build Command
```bash
cd /home/snow/asterinas
OSDK_LOCAL_DEV=1 cargo build --target aarch64-unknown-none-softfloat -p aster-nix
```

## Run Command
```bash
cd /home/snow/asterinas
rm -f /tmp/qemu_serial.log
OSDK_LOCAL_DEV=1 timeout 120 cargo osdk run --scheme aarch64 --target-arch aarch64
wc -l /tmp/qemu_serial.log && cat /tmp/qemu_serial.log
```

## Next Steps
1. Investigate why init process is stuck (check timer interrupts, serial output)
2. Add debug probes in kernel init to see where it stalls
3. Check if timer/IRQ handling works in user space
4. Consider simpler init (e.g., just run a simple shell command instead of full busybox init)

## Resume Instructions
- Read `.github/agent_state/2026-03-25-cleanup-checkpoint.md` first
- Build requires `OSDK_LOCAL_DEV=1` environment variable
- Current state: boots to "rootfs is ready" but init process stuck
