# Quickstart: RPi3 Hardware Bringup Validation

## Prerequisites

1. **Hardware**: Raspberry Pi 3 Model B (BCM2837, not RPi 4)
2. **SD card**: FAT-formatted with VideoCore firmware, U-Boot as `kernel8.img`, `config.txt`, `boot.scr`
3. **TFTP server**: Running on the dev machine, serving `asterina.img` and the uncompressed `initramfs.cpio` from `D:/pi_sd/` (mapped as `/mnt/d/pi_sd/` in WSL2). Do not use `/srv/tftp`.
4. **Serial console**: PL011 UART connection at 115200 baud, 8N1, to host `/dev/ttyUSB0`
5. **Build tools**: Docker with Asterinas aarch64-dev image

## Build

```bash
# 1. Build kernel
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest bash -c \
  'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64-rpi3'

# 2. Convert ELF → raw binary (required for booti)
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest bash -c \
  'OBJCOPY=~/.rustup/toolchains/nightly-2025-02-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objcopy
  $OBJCOPY -O binary \
    /root/asterinas/target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
    /root/asterinas/target/osdk/aster-nix/asterina.img'
# or not in Docker, with deploy
aarch64-linux-gnu-objcopy -O binary /home/snow/asterinas/target/aarch64-unknown-none-softfloat/release/aster-nix-osdk-bin /mnt/d/pi_sd/asterina.img

# 3. Deploy to the TFTP root on the dev machine
cp target/osdk/aster-nix/asterina.img /mnt/d/pi_sd/
# if the AArch64 initramfs is updated then
cp test/build/initramfs.cpio /mnt/d/pi_sd/initramfs.cpio
```

## Power

Power is software-controlled via the USB switch (power MCP): power off → clear the serial buffer → power on. No manual intervention needed.

## Verifier

```bash
# Capture serial output
# Don't change this command. If no message/no full msg/stop in "U-Boot>", just retry "power on->capture"
stty -F /dev/ttyUSB0 115200 raw -echo 2>/dev/null; cat /dev/ttyUSB0 &

# Power on / reset RPi3
# Expected: U-Boot loads → TFTP → asterina.img → boot messages → / # prompt

# Verify boot time: from power-on to / # should be < 60 seconds
```

## Expected Boot Log

```
Asterinas banner (Presented by the Asterinas developers, MPL-2.0)
=== AUTO-TEST-START ===
hello-from-init
--- ls / ---  (directory listing)
--- ls /bin ---  (applet listing)
=== AUTO-TEST-DONE ===
/ #
```

Note: The `/ #` shell prompt is the **correct and expected result**. If the kernel reaches `/ #` without crashing or hanging, the boot is successful. If QEMU exits immediately (exit code 0) without reaching `/ #`, the initramfs may be the wrong architecture (x86-64 instead of AArch64).

## RPi3 Timer Policy

- The RPi3 v1.1 bring-up uses the **BCM2836 non-secure physical timer** (`CNTPNSIRQ`, routed to ARM local IRQ 30) for a 1000 Hz periodic tick.
- The virtual timer (`CNTV_*`) is **not** enabled on RPi3; it is handled by the QEMU `virt` path instead.
- The driver lives in `ostd/src/arch/aarch64/timer/mod.rs` and the IRQ routing in `ostd/src/arch/aarch64/bcm2836_irq.rs`.
- `sleep 1` and scheduler `Waiter` timeouts are validated; do not re-enable the virtual timer for RPi3 until it is separately tested.

## v1.1 Build / Deploy / Smoke-Test Checklist

1. Build the AArch64 kernel and uncompressed AArch64 initramfs.
2. Convert `target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf` → `asterina.img` (raw AArch64 binary for `booti`).
3. Copy `asterina.img` and `test/build/initramfs.cpio` to `/mnt/d/pi_sd/` (the Windows TFTP root). Do not use `/srv/tftp`.
4. Ensure `boot.scr` on the SD card aborts and resets on any TFTP failure instead of booting stale data.
5. Power off the board, clear the serial buffer, power on, wait ~95 s (prompt lands ~50 s after power-on), and read serial until empty.
6. Record outcomes:
   - TFTP success/failure separately from kernel/user-space failure.
   - `/ #` prompt reached or hang point.
    - Shell commands: `echo hello`, `busybox sleep 1`, `ls /bin`, `busybox reboot -f` (applets live behind `busybox`; only echo/ls/cat/mkdir/mount/sh/true/umount are symlinked in /bin).
   - 10-cycle power baseline pass/fail count.

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
busybox nproc     # Expected: 4 (minimal procfs: /proc/interrupts is absent)
```

### 3. Interactive Shell
```bash
# Type: echo world
# Expected: world echoed back immediately
```

### 4. Reboot (SC-004)
```bash
busybox reboot -f
# Expected: PSCI reset, fresh firmware boot, back to / # in ~50 s
```

### 5. SMP (SC-005)
```bash
busybox nproc
# Expected: 4
busybox taskset -c 1 busybox echo ap1
# Expected: ap1 (proves APs schedule user threads)
```

## QEMU Regression Test

Use this to verify changes don't break QEMU virt boot:

```bash
qemu-system-aarch64 \
  -machine raspi3b -cpu cortex-a53 -smp 4 -m 1G \
  -dtb /mnt/d/pi_sd/bcm2710-rpi-3-b.dtb \
  -kernel /tmp/asterina.img -initrd test/build/initramfs.cpio \
  -append 'init=/init console=ttyAMA0' -nographic -display none -monitor none

# Expected: / # shell prompt in ~75 seconds
# NOTE: raspi3b QEMU has no EL3 firmware — PSCI paths are gated (psci_usable);
# use RPi3 hardware for PSCI-path validation.

### QEMU PSCI Note
QEMU raspi3b has no EL3 firmware, so PSCI SMC paths are skipped; `busybox reboot -f`
on QEMU will not reset the emulator. Hardware reset testing is RPi3-only
(`busybox reboot -f` verified → PSCI reset → prompt).

## Troubleshooting

| Symptom | Likely Cause |
|---------|-------------|
| No serial output after power-on | U-Boot not loading; check SD card, config.txt |
| Boot hangs before `/ #` prompt | Using wrong initramfs (x86-64 instead of AArch64); for QEMU use the uncompressed AArch64 `initramfs.cpio`; for RPi3 TFTP ensure `initramfs.cpio` is the AArch64 build. Also verify `boot.scr` aborts on TFTP errors |
| `reboot -f` returns to prompt or segfaults | Use `busybox reboot -f` (verified → PSCI reset). Plain `reboot` needs PID1 shutdown support (future work) |
| APs not scheduling | Check the image has the sevl/wfe/sev AP wait; verify with `busybox taskset -c 1 busybox echo ap1` |

## Operational Notes (C402)

- TFTP root is Windows `D:/pi_sd/` = WSL2 `/mnt/d/pi_sd/`; `/srv/tftp` is not used.
- `boot.scr`, DTB, and firmware files live on the SD card — SD updates require manual copy; ask the user to copy them.
- Power/serial are software-controlled (power MCP on COM3, serial MCP on COM7 @115200); workflow is power off → clear buffer → power on → wait ~95 s → read until empty.
- Failure taxonomy (C105): per-boot DHCP filename miss (`0A8E0F*.img` not found → falls back to `asterina.img`) is infra-normal; initramfs TFTP retries auto-reset clean; kernel faults (`EL1-SYNC`) and userspace failures are distinct classes.

## Further Documentation

Detailed documentation for RPi3 hardware bringup:

| Document | Description |
|----------|-------------|
| `test/rpi3/TFTP_BOOT_GUIDE.md` | Full TFTP boot procedure, SD card setup, U-Boot configuration |
| `test/rpi3/HARDWARE_TESTING_GUIDE.md` | Hardware testing procedures, serial console setup |
| `test/rpi3/TROUBLESHOOTING.md` | RPi3-specific troubleshooting |
| `test/rpi3/QEMU_RASPI3B_LIMITATIONS.md` | QEMU emulated RPi3 limitations vs real hardware |
