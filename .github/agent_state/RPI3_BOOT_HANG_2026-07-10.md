# RPi3 Boot Hang - Handoff (2026-07-10)

## Issue
RPi3 kernel hangs after "[a2-boot] calling ostd_main" - inside `crate::init()` before init markers appear.

## Root Causes Found

### 1. spin::Once::call_once() hangs on RPi3 (FIXED)
- `spin::Once::call_once()` hangs when called for `DEVICE_TREE` in early boot
- This prevented kernel from even reaching `crate::init()`
- **Fixed** by implementing `SimpleOnce` using `UnsafeCell<u8>` flag instead of `spin::Once`
- Files modified: `ostd/src/boot/mod.rs`, `ostd/src/arch/aarch64/boot/mod.rs`
- Commit: `eef399cb`

### 2. Kernel hangs inside crate::init() (INVESTIGATING)
- After "[a2-boot] calling ostd_main", no init markers (1,2,3...) appear
- `crate::init()` calls `early_marker(b'1')` first, but marker never prints
- Possible causes:
  - `early_marker` function issue
  - Memory/cache issue
  - Exception being caught silently

## Current Boot Progress (with SimpleOnce fix)
```
ABCDEFFGHI
[a2-boot] entry
[a2-boot] using loader dtb
[a2-boot] dtb discovery done
[a2-boot] initramfs found
[a2-boot] cmdline:  init=/init init=/init
[a2-boot] calling ostd_main
(HANGS - no init markers 1,2,3...)
```

## Key Files
- `ostd/src/arch/aarch64/boot/mod.rs`: SimpleOnce implementation, boot code
- `ostd/src/boot/mod.rs`: SimpleOnce re-export as Once<T>
- `ostd/src/lib.rs`: crate::init() with early_marker debug markers
- `ostd/src/arch/aarch64/trap/mod.rs`: Exception handlers

## Next Steps
1. Investigate why early_marker doesn't work inside `crate::init()`
2. Check if init() is crashing before first marker
3. Verify serial/MMIO setup is valid when init() runs
4. Consider adding pl011_puts at start of init() instead of early_marker

## Previous Working Commit
- `462cc567` - aarch64: implement PL011 UART RX interrupt for interactive shell
- Worked on 2026-07-03 but boot process has since changed
