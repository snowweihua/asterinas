# Asterinas RPi3 Bringup

This directory contains files and documentation for running Asterinas on Raspberry Pi 3 Model B / 3B+ hardware.

## Quick Links

| Document | Description |
|----------|-------------|
| [TFTP_BOOT_GUIDE.md](TFTP_BOOT_GUIDE.md) | Full TFTP boot procedure for rapid kernel iteration |
| [HARDWARE_TESTING_GUIDE.md](HARDWARE_TESTING_GUIDE.md) | Hardware testing procedures and serial console setup |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | RPi3-specific troubleshooting |
| [QEMU_RASPI3B_LIMITATIONS.md](QEMU_RASPI3B_LIMITATIONS.md) | QEMU emulated RPi3 vs real hardware |

## Quick Start

1. **Build kernel** (requires Docker with aarch64-dev image):
   ```bash
   docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest bash -c \
     'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64'
   ```

2. **Convert ELF to raw binary**:
   ```bash
   OBJCOPY=<path-to-llvm-objcopy>
   $OBJCOPY -O binary target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf target/osdk/aster-nix/kernel8_raw.bin
   ```

3. **Deploy**: Copy `kernel8_raw.bin` and `aarch64-shell-initramfs.cpio.gz` to your TFTP server root as `asterina.img` and `initramfs.cpio.gz`.

4. **Boot**: Power on RPi3 — U-Boot loads via TFTP and boots Asterinas.

5. **Verify**: Look for `/ #` shell prompt on serial console.

## Boot Flow

```
Power-on
  └── VideoCore GPU firmware (SD card: bootcode.bin, start.elf)
        └── U-Boot (SD card: kernel8.img = u-boot.bin)
              └── TFTP: load asterina.img + initramfs.cpio.gz
                    └── booti: jump to kernel
                          └── Asterinas: initramfs → shell
```

## Supported Hardware

- Raspberry Pi 3 Model B (BCM2837)
- Raspberry Pi 3 Model B+ (BCM2837B0)

Not supported: Raspberry Pi 4 (different architecture)

## Files

| File | Purpose |
|------|---------|
| `boot.cmd` / `boot.scr` | U-Boot boot script (TFTP load + booti) |
| `config.txt` | VideoCore config (PL011 UART, 64-bit mode) |
| `firmware/` | VideoCore firmware blobs (bootcode.bin, start.elf, fixup.dat) |
| `u-boot.bin` | U-Boot for RPi3B+ |
| `u-boot-rpi3b.bin` | U-Boot for RPi3B |
| `build_kernel_docker.sh` | Convenience script for Docker-based kernel builds |
| `copy_kernel.sh` | Convenience script for copying kernel to TFTP root |

## Key Fixes Implemented

- **D-cache flush**: `dc cvac` + `ic iallu` after MMU enable prevents first-boot hang
- **AP I-cache coherency**: `ic ivau` after copying AP boot stub prevents instruction cache staleness
- **PL011 UART RX interrupt**: Enables interactive shell on RPi3 hardware
- **Reboot syscall**: PSCI SYSTEM_RESET via syscall 88

## Known Issues

- `/bin/busybox ls /bin` causes SIGSEGV (stat struct layout mismatch with busybox binary)
- First boot may require reset on some RPi3 boards (D-cache coherency window)
- SMP (secondary cores) not yet operational
- QEMU RPi3 emulation has limitations — real hardware testing required for full validation
