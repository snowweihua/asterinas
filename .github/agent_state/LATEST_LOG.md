# Latest Log Pointer

- **Current checkpoint:** `2026-07-07-rpi3-boot-marker-i-hang.md`
- **Branch:** `aarch64_support`
- **Base commit:** `e08c4255` — "aarch64: fix RPi3 silent boot — UART address, MMU, platform detection"
- **Latest build:** `target/osdk/aster-nix/kernel8_raw.bin` (built 2026-07-07)
- **Latest RPi3 serial capture:** `ABCDEFFGHI` then hang after marker 'I'
- **Active blocker:** Kernel hangs after `detect_from_dtb_ptr()` / before/after `[a2-boot] entry\n` is visible on RPi3 serial.
- **Next hypothesis to test:** `BoardType::cached()` may be wrong (QEMU selected), or `pl011_puts_asm` 32-bit store fails on RPi3, or `Fdt::from_ptr` / `DEVICE_TREE.call_once` panics silently.
