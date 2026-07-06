# Feature Specification: RPi3 Hardware Bringup

**Feature Branch**: `aarch64_support`

**Created**: 2026-07-03

**Status**: Draft

**Input**: "bringup on real hardware RPi3B"

## User Scenarios & Testing *(mandatory)*

### Epic

As a kernel developer, I need Asterinas to reliably boot and run on Raspberry Pi 3 Model B hardware so I can develop and test kernel features in a real bare-metal environment.

---

### User Story 1 - Reliable First Boot (Priority: P1)

The kernel boots successfully on RPi3 hardware on every power-on or reset, without requiring manual intervention or retry.

**Why this priority**: First-boot reliability is a blocking issue — every power cycle that fails adds minutes of debug overhead and breaks development flow.

**Independent Test**: Can be tested by power-cycling the RPi3 10 consecutive times and observing whether the kernel reaches the shell prompt on each attempt.

**Acceptance Scenarios**:

1. **Given** RPi3 is powered off, **When** power is applied and U-Boot loads the kernel, **Then** the kernel boots to a shell prompt without requiring a manual reset.
2. **Given** RPi3 is already running Asterinas, **When** the reboot syscall is invoked or hardware reset occurs, **Then** the kernel reboots cleanly and reaches the shell prompt within 60 seconds.

---

### User Story 2 - Stable Shell Interaction (Priority: P1)

The interactive shell on RPi3 serial console runs common utilities without crashing.

**Why this priority**: A crashing shell on basic commands blocks all further development and testing on hardware.

**Independent Test**: Can be tested by running `ls /bin` and `echo hello` via serial console and verifying no crash occurs.

**Acceptance Scenarios**:

1. **Given** kernel has booted to shell prompt on RPi3, **When** `ls /bin` is executed, **Then** the command returns without SIGSEGV and lists directory contents.
2. **Given** kernel has booted to shell prompt on RPi3, **When** `echo hello` is executed, **Then** `hello` is echoed back.
3. **Given** kernel has booted to shell prompt on RPi3, **When** `cat /proc/interrupts` is executed, **Then** interrupt counts are displayed including serial and timer IRQs.

---

### User Story 3 - SMP Secondary Core Bringup (Priority: P2)

Secondary CPU cores on RPi3 are successfully brought online via PSCI and can run kernel code.

**Why this priority**: SMP is needed for multi-core testing and parallel kernel operations. Currently deferred.

**Independent Test**: Can be tested by checking `/proc/cpuinfo` or equivalent for multiple online CPUs after boot.

**Acceptance Scenarios**:

1. **Given** kernel has booted on RPi3, **When** the SMP initialization path executes, **Then** at least 2 CPU cores are detected as online.
2. **Given** secondary cores are online, **When** a kernel thread is scheduled across cores, **Then** no crashes or sync errors occur.

---

### User Story 4 - Clean Reboot (Priority: P3)

The reboot syscall terminates all processes cleanly and reboots the hardware without hanging.

**Why this priority**: Without a working reboot, the only way to restart is power cycling — slow and inconvenient during development.

**Independent Test**: Can be tested by invoking the reboot syscall and observing hardware reset within 30 seconds.

**Acceptance Scenarios**:

1. **Given** kernel is running on RPi3, **When** the reboot syscall (169) is invoked, **Then** the system reboots within 30 seconds without kernel panic.

---

### Edge Cases

- UART serial connection drops during boot — kernel should not hang, should continue waiting for serial connection.
- Initramfs CPIO extraction fails on first boot but succeeds on subsequent boot — should eventually succeed without manual intervention.
- Secondary core fails to start via PSCI — kernel should continue with available cores and log the failure.
- Watchdog timer fires during extended boot — watchdog should be kicked as long as the scheduler is running.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Kernel MUST boot to shell prompt on RPi3 3B hardware within 60 seconds of power-on
- **FR-002**: `ls /bin` and `echo` commands MUST execute without SIGSEGV on RPi3 serial console
- **FR-003**: Timer IRQs (CNTV) MUST fire continuously to keep the scheduler alive
- **FR-004**: PL011 UART RX interrupt MUST deliver typed characters to the shell
- **FR-005**: PSCI reboot MUST reset all CPU cores cleanly without hanging
- **FR-006**: SMP CPU_ON for secondary cores MUST succeed and report cores as online
- **FR-007**: D-cache and I-cache MUST be coherent after CPIO extraction and before entering user space

### Key Entities *(include if feature involves data)*

- **RPi3 3B SoC**: BCM2837, 4x Cortex-A53 cores, ARM Generic Timer (CNTV), BCM2835 peripheral framework (GPU -> CPU IRQ router)
- **PL011 UART**: Serial console at 115200 baud, 8N1, mapped at GPIO 14/15
- **BCM2836 IRQ Router**: Routes peripheral IRQs (timer, UART, USB) to appropriate CPU cores
- **PSCI**: Power State Coordination Interface for CPU power management and SMP bringup
- **Initramfs**: CPIO archive loaded by U-Boot via TFTP, passed to kernel via DTB `linux,initrd-start/end`
- **Two-stage boot**: VideoCore firmware loads U-Boot (as `kernel8.img`) from SD card; U-Boot then loads Asterinas via TFTP and boots using `booti`

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Boot success rate on RPi3 3B reaches 100% (10/10 consecutive power cycles)
- **SC-002**: Time from power-on to shell prompt averages under 30 seconds over 10 attempts
- **SC-003**: `ls /bin` succeeds without SIGSEGV in 10/10 attempts on RPi3 hardware
- **SC-004**: Reboot completes within 30 seconds with no kernel panic
- **SC-005**: At least 2 CPU cores detected online on RPi3 after SMP bringup
- **SC-006**: No watchdog reset occurs during normal operation (scheduler keeps timer IRQ alive)

## Assumptions

- Two-stage boot: VideoCore firmware (start.elf, etc.) on the SD card loads `kernel8.img` (U-Boot binary) and executes it; U-Boot then loads `asterina.img` (raw AArch64 binary from `llvm-objcopy -O binary`) via TFTP and boots it with `booti`
- The SD card FAT partition contains VideoCore firmware files, `kernel8.img` (U-Boot), `config.txt`, and `boot.scr`
- U-Boot is pre-configured with TFTP boot commands; the boot script (`boot.scr`) loads `asterina.img` and `initramfs.cpio.gz` via TFTP and passes them via DTB `linux,initrd-start/end`
- Serial console is connected via PL011 UART at 115200 baud, 8N1, to a host machine
- A TFTP server is running on the host machine serving `asterina.img` and `initramfs.cpio.gz`
- `config.txt` on the SD card has `arm_64bit=1` and `kernel=kernel8.img` (VideoCore loads U-Boot as first-stage kernel)
- RPi3 3B revision (BCM2837) is used — not RPi 4 or later
- The existing QEMU virt emulation is the primary development iteration target; RPi3 hardware is the final validation step
