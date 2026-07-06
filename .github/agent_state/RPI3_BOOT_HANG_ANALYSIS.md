# RPi3 Boot Hang - Analysis Checkpoint

**Date**: 2026-07-06
**Status**: Kernel hangs silently after "Starting kernel ..." — NO serial output at all

## Symptoms

1. U-Boot successfully loads kernel (3.4 MiB transferred, "Image lacks image_size field" indicates ARM64 Image header validated)
2. U-Boot prints "Starting kernel ..."
3. Kernel hangs — no boot messages, no shell prompt, nothing
4. Same behavior across multiple boot attempts

## Root Cause Analysis

### Committed Code Issues

The committed code HARDCODES QEMU UART address (0x09000000) in TWO places:

1. **`ostd/src/arch/aarch64/boot/boot.S`** — `early_uart_putchar` macro:
   ```asm
   ldr x1, =0x09000000  ; WRONG for RPi3 (should be 0x3F215030)
   ```

2. **`ostd/src/arch/aarch64/boot/mod.rs`** — `pl011_puts_asm`:
   ```asm
   mov x2, #0x09000000  ; WRONG for RPi3
   ```

3. **`ostd/src/arch/aarch64/serial.rs`** — Uses only QEMU PA:
   ```rust
   const PL011_BASE_PA: usize = 0x0900_0000;
   const PL011_BASE_VA: usize = 0xffff_8000_0900_0000;
   ```

### Key Observation

**There is NO early debug output in boot.S before `_start_real`!**

The `early_uart_putchar` macro is defined but NEVER CALLED. So even if we fixed the UART address, there's no output to see.

The first opportunity for output is in `aarch64_boot()` in `mod.rs` which calls `pl011_puts()` — but this uses the WRONG UART address.

## Fixes Applied (Uncommitted)

1. **boot.S**: Added PC-based board detection to `early_uart_putchar`:
   - If PC < 0x1000000 → RPi3 → UART = 0x3F215030
   - If PC >= 0x1000000 → QEMU → UART = 0x09000000
   - Added `early_uart_putchar 'K'` at `_start_real` for verification

2. **mod.rs**: Changed `pl011_puts_asm` to accept UART base as 3rd parameter

3. **serial.rs**: Made board-aware using `BoardType::cached()`

## Debugging Tried

1. Added `early_uart_putchar 'K'` at `_start_real` → **NO 'K' output**
2. Tried `dsb sy` barriers after store → no change
3. Tried UARTCR init (UARTEN bit) → no change
4. Tried different board detection thresholds → no change

## Possible Causes for "No Output" Even After Fix

1. **Kernel not reaching _start_real**: booti might be failing silently
2. **Kernel crashing before output**: Unhandled exception/replication
3. **Output is being produced but lost**: UART TX not connected properly
4. **Initramfs issue**: Kernel hangs during CPIO extraction (but no output even before that)

## Next Debug Steps (Systematic)

### Step 1: Verify kernel entry
Add debug output BEFORE the Image header jump (at offset 0 in the binary) to confirm booti transfers control correctly.

### Step 2: Verify UART is accessible
Write to UART DR directly (no FR check) and see if any output appears.

### Step 3: Check if kernel is actually running
Add a delay loop or periodic output to see if kernel is running at all.

### Step 4: Compare with working commit
The AGENTS.md says boot worked on 2026-07-03. We need to find what commit that was and compare.

## Files Modified

- `ostd/src/arch/aarch64/boot/boot.S`
- `ostd/src/arch/aarch64/boot/mod.rs`
- `ostd/src/arch/aarch64/mod.rs`
- `ostd/src/arch/aarch64/serial.rs`

## TFTP Deploy Path
**IMPORTANT**: Kernel must be copied to `/mnt/d/pi_sd/asterina.img` (NOT `/mnt/d/asterina.img`)
