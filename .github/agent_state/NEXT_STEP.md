# Next Step — 2026-07-07

## Immediate Problem

Kernel reaches all early boot markers `ABCDEFFGHI` on real RPi3 3B hardware but hangs silently immediately after marker 'I'.  The next expected output (`[a2-boot] entry\n` or later init messages) is never seen.

## Fix Strategy

### Step 1: Confirm execution passes marker 'I'
Add a `J` marker immediately after the `detect_from_dtb_ptr()` call in `aarch64_boot()` and before `pl011_puts(b"[a2-boot] entry\n")`.

If 'J' is printed, the problem is in or after `pl011_puts()`.  If 'J' is not printed, the problem is inside `detect_from_dtb_ptr()` / `BoardType::cache()` / `early_marker()` itself.

### Step 2: Print the cached board type
Add a tiny raw-hex output helper (or just emit `0` / `1` / `2` markers) to show `BoardType::cached()` value:
- `0` = unknown
- `1` = QEMU virt
- `2` = Raspberry Pi 3

If the value is `1`, the DTB byte-scan failed to find `bcm2837` and we are writing to the wrong UART address.

### Step 3: Verify PL011 store width
If the board type is `2` but still no output, change `pl011_puts_asm` to use `strb w3, [x2]` instead of `str w3, [x2]` to match the working `early_marker()` store width.

### Step 4: Check for panic between 'I' and 'J'
If neither 'J' nor the board-type marker appears, the issue may be a silent panic in `Fdt::from_ptr()` (its internal `.unwrap()` on `FdtHeader::from_bytes`) or in `DEVICE_TREE.call_once(|| fdt)` (unlikely at runtime).  Add a `J` marker before the `Fdt::from_ptr` call and a `K` marker after `DEVICE_TREE_REGION` is stored.

### Step 5: If all else fails, instrument the exception handler
The exception handler already writes to both UARTs.  Verify it is actually being reached by deliberately causing a fault from a known point and confirming the dump appears.

## Key Files

- `ostd/src/arch/aarch64/boot/mod.rs` — `aarch64_boot()`, `pl011_puts_asm()`, `early_marker()`, `is_valid_dtb_paddr()`, `discover_dtb_paddr()`.
- `ostd/src/arch/aarch64/board.rs` — `detect_from_dtb_ptr()`, `BoardType::cache()`, `BoardType::cached()`.
- `ostd/src/arch/aarch64/trap/mod.rs` — early exception handler for faults.
- `AGENTS.md` — update status after the fix.

## Build / Deploy Commands

```bash
# Build (Docker)
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64

# Convert to raw binary
OBJCOPY=$(find ~/.rustup -name "llvm-objcopy" 2>/dev/null | head -1)
$OBJCOPY -O binary \
  target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  target/osdk/aster-nix/kernel8_raw.bin
od -An -tx1 -j0x38 -N4 target/osdk/aster-nix/kernel8_raw.bin

# Deploy to TFTP root on SD card
cp target/osdk/aster-nix/kernel8_raw.bin /mnt/d/pi_sd/asterina.img

# Capture serial
stty -F /dev/ttyUSB0 115200 raw -echo && timeout 180 cat /dev/ttyUSB0
# Power-cycle RPi3
```

## Ultimate Goal

Get the RPi3 3B to print `[a2-boot] entry` and continue booting to the user-space shell.
