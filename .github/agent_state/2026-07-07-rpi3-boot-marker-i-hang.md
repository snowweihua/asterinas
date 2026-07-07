# RPi3 Boot Progress - Hang After Marker 'I'

**Date:** 2026-07-07  
**Branch:** `aarch64_support`  
**Commit base:** `e08c4255` ("aarch64: fix RPi3 silent boot — UART address, MMU, platform detection")

## Summary

Kernel now reaches all early boot markers `ABCDEFFGHI` on real RPi3 3B hardware, but hangs silently immediately after the 'I' marker. The issue is between `detect_from_dtb_ptr()` (marker 'I') and the first `[a2-boot] entry
` line (or immediately after it). We have not yet reached a shell prompt.

## Last Verified Behavior

**Build:**
```bash
# Docker build (must rebuild after source edits)
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64

# Convert to raw binary
OBJCOPY=$(find ~/.rustup -name "llvm-objcopy" 2>/dev/null | head -1)
$OBJCOPY -O binary \
  target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  target/osdk/aster-nix/kernel8_raw.bin

cp target/osdk/aster-nix/kernel8_raw.bin /mnt/d/pi_sd/asterina.img
```

**Deploy / capture:**
```bash
stty -F /dev/ttyUSB0 115200 raw -echo && timeout 180 cat /dev/ttyUSB0
# Power-cycle RPi3
```

**Key output markers observed:**
```
Starting kernel ...
ABCDEFFGHI
(timeout)
```

The first `A` (marker 'A') comes from `boot.S` before `jump_to_main`.  `B` from `boot.S` after EL switch. `C` and `D` from `_start_virt` in `boot.S`.  `E` from the first instruction in `aarch64_boot` in `boot/mod.rs`. Second `F` after `trap::init()` in `aarch64_boot`. `G` after `crate::arch::trap::init()`. `H` after `discover_dtb_paddr()`. `I` after `detect_from_dtb_ptr()`.

## Files Touched

- `ostd/src/arch/aarch64/boot/boot.S` — early markers A-D/F, VBAR_EL1 early setup, EL switch, SP fixup, PC-based DRAM detection.
- `ostd/src/arch/aarch64/boot/mod.rs` — marker 'E/F/G/H/I', `early_marker()`, `pl011_puts_asm` (now no longer busy-waits on FR.TXFF), `is_valid_dtb_paddr` (removed strict `Fdt::from_ptr` validation), fatal-message handling around `Fdt::from_ptr` unwrap.
- `ostd/src/arch/aarch64/board.rs` — `detect_from_dtb_ptr()` now scans raw DTB bytes for `b"bcm2837"` / `b"raspberrypi"` instead of parsing the FDT via `fdt::Fdt::from_ptr()`.
- `ostd/src/arch/aarch64/trap/mod.rs` — exception handler now writes diagnostic text to both RPi3 and QEMU UART addresses without board detection.
- `AGENTS.md` — status date updated.
- `specs/001-rpi3-hardware-bringup/tasks.md` — T002 sub-notes.
- `.gitignore` — added local caches: `.cargo/`, `.docker-cargo/`, `.omo/`, `.opencode/`.

## Known Facts

1. `Fdt::from_ptr` in `fdt` crate v0.1.5 internally calls `FdtHeader::from_bytes(...).unwrap().totalsize.get()`.  There is no `BadVersion` variant; it only checks magic / null / buffer size.
2. The RPi3 DTB (relocated by U-Boot to `0x2600000`, total size ~46 974 bytes) has valid magic and bounds.
3. We currently removed strict FDT validation in `is_valid_dtb_paddr` and only rely on magic + size bounds.
4. `detect_from_dtb_ptr()` uses `core::slice::from_raw_parts` on the raw DTB.  It should identify the board as `RaspberryPi3` if `bcm2837` is present.
5. After marker 'I', the first call is `pl011_puts(b"[a2-boot] entry\n")` which uses `early_uart_base()` -> `BoardType::cached()` -> PL011 DR at `0x3F201000` on RPi3.
6. The output of `pl011_puts` is **not observed** on the RPi3 serial console.

## Hypotheses (in order of likelihood)

1. **Board cache is wrong** — `detect_from_dtb_ptr()` is defaulting to `QemuVirt` (because scan didn't find `bcm2837`), so `early_uart_base()` returns QEMU address `0x09000000` and the output goes to the wrong UART.
2. **Fdt / slice memory access is silently bad** — `detect_from_dtb_ptr()` reads through the boot identity map.  If the pointer is invalid, an abort might occur, but our exception handler should print.  No dump is observed, so either the abort is caught and the handler itself hangs, or no abort occurs.
3. **PL011 data-register store is not working** — although the raw `strb` in `early_marker()` works, the `str w3, [x2]` in `pl011_puts_asm` might behave differently (e.g., 32-bit store vs 8-bit, FIFO state, or the PL011 data register is 32-bit and only the lower 8 bits matter).  The 32-bit write should work, but this is untested on the RPi3 BCM2837 PL011.
4. **Something panics after marker 'I'** — `Fdt::from_ptr` could be panicking inside (its `.unwrap()`), or `DEVICE_TREE.call_once(|| fdt)` could be panicking because of a lifetime/static mismatch.  The panic handler on AArch64 loops silently, so we would see nothing.
5. **Output is there but lost / FIFO full** — `pl011_puts_asm` was changed to not poll `FR.TXFF`.  If the FIFO was already full, the 16-byte string would be dropped.  However, the FIFO should have drained during the time between markers.

## Next Concrete Steps

1. **Add more granular markers** inside `aarch64_boot` between 'I' and `[a2-boot] entry
`:
   - Marker 'J' immediately before `pl011_puts(b"[a2-boot] entry\n")`.
   - Marker 'K' immediately after `pl011_puts(...)`.
   - Marker 'L' after `DEVICE_TREE.call_once(...)`.
2. **Print the cached board type** in hex using `early_marker()` or a tiny raw-hex routine, so we can see whether `BoardType::cached()` is 2 (RPi3) or 1 (QEMU).
3. **Test whether the 32-bit PL011 store works** by changing `pl011_puts_asm` to use `strb` instead of `str` and seeing if `[a2-boot] entry
` appears.
4. **If board cache is wrong**, instrument `detect_from_dtb_ptr()` to emit a marker when it finds `bcm2837`, and/or dump the first 16 bytes of the DTB.
5. **If the issue is a panic**, wrap the `Fdt::from_ptr(...)` call and any `DEVICE_TREE` storage in a custom panic hook that prints raw text before looping.  Alternatively, avoid calling `Fdt::from_ptr` entirely until later (we only need board detection and raw DTB bytes for the initramfs).

## Resume Here

If this session is resumed, run:
```bash
cd /home/snow/asterinas
git status
# Verify branch is aarch64_support
./tools/format_all.sh --check
# Add the next markers (J/K/L) as described above, then rebuild + deploy.
```

Current binary artifact is at `target/osdk/aster-nix/kernel8_raw.bin` and was deployed to `/mnt/d/pi_sd/asterina.img`.
