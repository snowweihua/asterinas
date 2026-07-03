# QEMU raspi3b Limitations for RPi3 Testing

## Overview

The `qemu-system-aarch64 -machine raspi3b` target provides limited emulation of the Raspberry Pi 3B hardware. It is useful for verifying early boot code (boot.S, page tables, MMU enable) but cannot validate most peripherals or interrupts.

## What Works

- **Boot code execution**: QEMU loads `kernel8.img` at PA 0x80000 and starts execution
- **ARM64 Linux Image header**: Recognized by QEMU, DTB passed in x0
- **Basic MMU enable**: Page table setup can be verified if kernel doesn't crash immediately
- **PL011 UART (partial)**: QEMU raspi3b emulates a PL011 at 0x3F201000, NOT the mini-UART at 0x3F215000

## What Does NOT Work

- **BCM2836 Local Interrupt Controller**: Not emulated. Timer interrupts will not fire.
- **mini-UART (GPIO 14/15)**: Not emulated. Early debug output via mini-UART will not appear.
- **ARM Generic Timer interrupts**: Not routed through BCM2836 Local IC.
- **SD card / eMMC**: Not emulated in a way useful for OS development.
- **USB, Ethernet, WiFi, Bluetooth**: Not emulated.
- **GPU / VideoCore**: Not emulated.
- **Device tree**: QEMU generates its own DTB which differs from real RPi3 firmware DTB.

## Testing Strategy

1. **Use QEMU raspi3b for**: Boot.S validation, page table verification, early MMU bring-up
2. **Use real hardware for**: Timer interrupts, serial output (mini-UART), full peripheral validation, stress tests

## Known QEMU Behavior

- QEMU raspi3b may hang silently if the kernel accesses unemulated peripherals
- No serial output from mini-UART means early_puts on RPi3 won't be visible in QEMU
- The generated DTB has different compatible strings than real RPi3 firmware

## Recommendations

- Always test on real RPi3 hardware before declaring a milestone complete
- Use QEMU raspi3b only as a smoke test for build/link correctness
- For automated CI, stick to QEMU virt machine (which has full GIC and PL011 emulation)
