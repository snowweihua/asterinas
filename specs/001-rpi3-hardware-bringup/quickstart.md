# Quickstart: RPi3 Hardware Bringup Validation

## Prerequisites

1. **Hardware**: Raspberry Pi 3 Model B (BCM2837, not RPi 4)
2. **SD card**: FAT-formatted with VideoCore firmware, U-Boot as `kernel8.img`, `config.txt`, `boot.scr`
3. **TFTP server**: Running on host machine, serving `asterina.img` and `initramfs.cpio.gz`
4. **Serial console**: PL011 UART connection at 115200 baud, 8N1, to host `/dev/ttyUSB0`
5. **Build tools**: Docker with Asterinas aarch64-dev image

## Build

```bash
# 1. Build kernel
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest bash -c \
  'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64'

# 2. Convert ELF → raw binary (required for booti)
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest bash -c \
  'OBJCOPY=~/.rustup/toolchains/nightly-2025-02-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objcopy
  $OBJCOPY -O binary \
    /root/asterinas/target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
    /root/asterinas/target/osdk/aster-nix/asterina.img'

# 3. Deploy to TFTP root
cp target/osdk/aster-nix/asterina.img /mnt/d/pi_sd/
cp test/build/initramfs.cpio.gz /mnt/d/pi_sd/
```

## Boot Test (Manual)

```bash
# Connect serial monitor
stty -F /dev/ttyUSB0 115200 raw -echo
cat /dev/ttyUSB0 &
# Or: screen /dev/ttyUSB0 115200

# Power on / reset RPi3
# Expected: U-Boot loads → TFTP → asterina.img → boot messages → / # prompt

# Verify boot time: from power-on to / # should be < 30 seconds
```

## Expected Boot Log

```
[asterinas] boot
[kernel] rootfs is ready
[drv] setup uart irq=57
[kernel] sched init
/task-loop/ top-of-loop
[kt1] init in first kthread
... boot messages ...
/ #
```

## Test Scenarios

### 1. Reliable First Boot (SC-001)
```bash
# Power off RPi3 completely, wait 5 seconds, power on
# Repeat 10 times
# Pass: 10/10 boots reach / # prompt
```

### 2. Basic Shell Commands (SC-003)
```bash
echo hello        # Expected: hello
ls /bin          # Expected: list of binaries (no SIGSEGV)
cat /proc/interrupts  # Expected: interrupt counts
```

### 3. Interactive Shell
```bash
# Type: echo world
# Expected: world echoed back immediately
```

### 4. Reboot (SC-004)
```bash
reboot
# Expected: system resets within 30 seconds, boots back to / #
```

### 5. SMP (SC-005) — after FR-006 is implemented
```bash
cat /proc/cpuinfo
# Expected: 4x CPU online
```

## QEMU Regression Test

Use this to verify changes don't break QEMU virt boot:

```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -smp 1 -m 512M \
  -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  -dtb test/nix/aarch64-virt.dtb \
  -device loader,file=test/build/virt-init.dtb,addr=0x47000000,force-raw=on \
  -device loader,file=test/build/init.cpio.gz,addr=0x48000000,force-raw=on \
  -append "console=ttyAMA0" -nographic -display none

# Expected: / # prompt within 60 seconds
```

## Troubleshooting

| Symptom | Likely Cause |
|---------|-------------|
| No serial output after power-on | U-Boot not loading; check SD card, config.txt |
| Boot hangs at `[kt1] init in first kthread` | Initramfs CPIO extraction failed; D-cache coherency issue |
| `ls /bin` causes SIGSEGV | stat struct layout mismatch (FR-002) |
| Reboot hangs | reboot syscall 169 not implemented |
| APs not coming online | BCM2836 spin-table SMP issue (FR-006) |
