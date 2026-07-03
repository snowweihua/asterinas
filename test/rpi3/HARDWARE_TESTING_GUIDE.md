# RPi3 Hardware Testing Guide

## Prerequisites

- Raspberry Pi 3B or 3B+
- MicroSD card (8GB+)
- TTL-USB serial cable (3.3V logic level)
- Computer with USB port for serial capture

## Wiring

Connect TTL-USB serial cable to RPi3 GPIO header:
- GPIO 14 (TXD) → RX on TTL cable
- GPIO 15 (RXD) → TX on TTL cable
- GND → GND on TTL cable

DO NOT connect 5V from TTL cable to RPi3.

## Required Files on SD Card

The RPi3 GPU (VideoCore IV) boots first and needs proprietary firmware blobs:

| File | Purpose | Source |
|------|---------|--------|
| `bootcode.bin` | GPU bootloader | Raspberry Pi firmware |
| `start.elf` | GPU firmware, reads config.txt | Raspberry Pi firmware |
| `fixup.dat` | Memory split configuration | Raspberry Pi firmware |
| `config.txt` | Boot configuration | Provided in test/rpi3/ |
| `cmdline.txt` | Kernel command line | Provided in test/rpi3/ |
| `kernel8.img` | Asterinas kernel | Built from source |

**IMPORTANT:** `kernel8.img` alone is NOT enough. The GPU firmware files are required.

## Build Instructions

```bash
# 1. Build kernel with RPi3 scheme
cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3

# 2. Convert ELF to raw binary (kernel8.img)
# Find rust-objcopy path:
find /root/.rustup -name rust-objcopy 2>/dev/null

# Use the found path:
/root/.rustup/toolchains/nightly-2025-02-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-objcopy \
  -O binary \
  target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  target/osdk/aster-nix/kernel8.img

# 3. Download GPU firmware files (bootcode.bin, start.elf, fixup.dat)
cd test/rpi3
./download_firmware.sh

# 4. Prepare SD card (includes firmware + config + kernel)
sudo ./build_boot_sd.sh /dev/sdX ../../target/osdk/aster-nix/kernel8.img
```

## Serial Capture

```bash
# Install minicom or screen
sudo apt-get install minicom

# Connect at 115200 baud, 8N1
minicom -D /dev/ttyUSB0 -b 115200

# Or use screen:
screen /dev/ttyUSB0 115200
```

## Expected Boot Sequence

If successful, you should see:

```
[a2-boot] entry
[a2-boot] using loader dtb
[a2-boot] dtb discovery done
[a2-boot] initramfs found
[a2-boot] cmdline: ...
[a2-boot] calling ostd_main
[kernel] OSTD initialized. Preparing components.
[kernel] Spawn init thread
...
[Asterinas banner]
/ # 
```

## Milestones

| Milestone | Expected Output | What It Verifies |
|-----------|----------------|------------------|
| M1 | `[a2-boot] entry` | boot.S executes, PL011 UART works |
| M2 | `[a2-boot] dtb discovery done` | DTB parsing works |
| M3 | `Asterinas` banner | Kernel boots to main |
| M4 | `/ #` prompt | Shell is ready |
| M5 | Timer tick messages | BCM2836 timer IRQ works |

## Troubleshooting

### No output at all
- Check serial cable wiring (TXD/RXD may be swapped)
- Verify `enable_uart=1` in config.txt
- Check baud rate is 115200
- Try different serial cable

### Garbled output
- Check baud rate (must be 115200)
- Verify ground connection
- Try shorter serial cable

### Hangs after `[a2-boot] entry`
- FDT magic detection may have failed
- Board detection may have defaulted to QEMU
- Check if DTB pointer in x0 is valid

### Hangs after `[a2-boot] calling ostd_main`
- MMU enable may have failed
- Page table setup may be wrong
- SP fix-up may be incorrect

### No timer interrupts
- BCM2836 register offsets may be wrong
- Timer IRQ enable may have failed
- Check `bcm2836_irq::enable_timer_irq()` is called

## Debug Tips

To add more debug output, edit `ostd/src/arch/aarch64/boot/mod.rs`:
```rust
unsafe { early_puts(b"[debug] message here\n") };
```

Rebuild and retest.

## Reporting Issues

If testing fails, capture:
1. Full serial output (from power-on)
2. `git log --oneline -5` output
3. Build command used
4. RPi3 model (3B or 3B+)
