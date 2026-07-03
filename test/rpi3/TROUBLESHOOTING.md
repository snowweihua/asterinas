# RPi3 Hardware Troubleshooting Guide

## Expected LED Behavior (Latest kernel8.img)

With the latest kernel8.img, you should see this EXACT pattern on the **green ACT LED**:

1. **5 quick blinks** (immediately after power-on) - Assembly code in boot.S running
2. **Short pause** (~1 second)
3. **3 quick blinks** - MMU enabled successfully  
4. **Short pause** (~1 second)
5. **5 more blinks** - Reached Rust code (aarch64_boot)

**Total: 13 blinks in groups of 5-3-5**

## If You See NO Blinks At All

This means the kernel code is NOT executing. Check these in order:

### 1. SD Card Format
```bash
# Verify SD card has FAT32 boot partition
sudo fdisk -l /dev/sdX
# Should show:
# /dev/sdX1  W95 FAT32 (LBA)  256M  (boot partition)
# /dev/sdX2  Linux            rest  (root partition)
```

### 2. Files on Boot Partition
Mount the boot partition and verify ALL files exist:
```bash
sudo mount /dev/sdX1 /mnt
ls -la /mnt/
# Should show:
# bootcode.bin  (GPU bootloader)
# start.elf     (GPU firmware)
# fixup.dat     (Memory config)
# config.txt    (Boot configuration)
# cmdline.txt   (Kernel command line)
# kernel8.img   (Asterinas kernel)
sudo umount /mnt
```

### 3. kernel8.img Size and Timestamp
```bash
ls -la /mnt/kernel8.img
# Should be ~3.5MB and recent timestamp
# If it's 0 bytes or very old, rebuild:
cd /home/snow/asterinas
docker run --rm -m 2g -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3"
```

### 4. config.txt Contents
```bash
cat /mnt/config.txt
# MUST contain:
# arm_64bit=1
# enable_uart=1
# dtoverlay=disable-bt
# kernel=kernel8.img
```

### 5. Power Supply
- RPi3 needs 5V/2.5A minimum
- Weak power causes boot failures
- Try a different power supply or USB cable

### 6. Serial Cable (for additional debugging)
Even if LED blinks, serial output helps:
- **Baud rate: 115200** (not 9600!)
- **GPIO 14 (TXD) → RX on TTL cable**
- **GPIO 15 (RXD) → TX on TTL cable**
- **GND → GND**
- Use 3.3V logic level (NOT 5V!)

## If You See Only 5 Blinks (First Group Only)

This means the kernel loaded but hangs before MMU enable:
- Check that `kernel8.img` was built with the RPi3 scheme (not QEMU)
- Verify the linker script uses `KERNEL_LMA = 0x80000`

## If You See 8 Blinks (5 + 3, No Third Group)

This means MMU enabled but hang before reaching Rust code:
- Page table setup may be wrong
- Check boot.S page table patching for RPi3 DRAM at 0x0

## Emergency Debug: Force LED On

If you want to test if the RPi3 can execute ANY code from kernel8.img:

Create a minimal test that just turns LED on solid:

```bash
# Create minimal test binary (just turns LED on)
printf '\x00\x00\x00\x14' > /tmp/led_on.bin  # b . (infinite loop)
# This won't actually work - need proper ARM64 code
```

Better: Use the existing kernel8.img but watch for ANY LED activity (even faint/flickering).

## Quick Verification Checklist

- [ ] SD card is 8GB or larger
- [ ] Boot partition is FAT32
- [ ] All 6 files present on boot partition
- [ ] kernel8.img is ~3.5MB
- [ ] config.txt has `arm_64bit=1`
- [ ] config.txt has `kernel=kernel8.img`
- [ ] Using 5V/2.5A power supply
- [ ] Green LED shows activity (not just solid)

## Reporting Issues

If still not working, provide:
1. Exact LED behavior (number of blinks, pattern, timing)
2. `ls -la` output of boot partition
3. `xxd /mnt/kernel8.img | head -5` output
4. Photo of serial cable wiring (if applicable)
5. Power supply rating
