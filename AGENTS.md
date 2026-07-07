# AArch64 Development Agent Instructions

## Development Environment

Requires Docker image `asterinas/aarch64-dev:latest` with QEMU, KVM, Nix, and OSDK tools pre-installed.

**Docker build (when local build fails due to permissions):**
```bash
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64
```

**Important - KVM limitation on WSL2:**
- Docker on WSL2 cannot use KVM acceleration (WSL2 doesn't expose `/dev/kvm` to containers)
- For QEMU tests on WSL2, either:
  - Run QEMU directly on host (not in Docker) if KVM is available
  - Or accept slow TCG emulation mode
- CI on GitHub Actions works fine with KVM (runs on native Ubuntu)

## Agent Session State

**Git Workflow:**
- Always work on a named branch (e.g., `aarch64_support`), not detached HEAD
- Commits on detached HEAD are lost when switching branches
- Use `git stash` sparingly; commits are safer for persistence
- Reflog (`git reflog`) can recover "lost" commits: `git reflog | grep <keyword>`
- Cherry-pick from reflog to restore: `git cherry-pick <commit-hash>`

**Commit/Push Frequency:**
- Do NOT commit and push after every action - commit when work is complete or paused
- User will ask for commit/push when needed

**Checkpoints:**
- Use `.github/agent_state/` for recovery checkpoints
- Use `target/agent_logs/` for run logs

## Current Status (2026-07-23)

**Completed:** P0.1, P0.2, P1.1, P1.2, P1.3, P2.2, P3.1, P4.1, P4.2, P5.1 (RPi3 hardware),
initramfs hang fix, VirtIO init fix, user-space shell (QEMU + RPi3), timer IRQs (QEMU + RPi3),
PL011 UART RX interrupt (QEMU + RPi3), exception table for fallible user-memory access,
RPi3 silent boot fix (UART address, MMU, platform detection)

**Latest commits (2026-07-23) — RPi3 silent boot fixes:**
- boot.S: text_offset=0x80000, PC-based DRAM_BASE detection (RPi3=0 / QEMU=0x40000000),
  dynamic boot_l3pt_high patch, RPi3 peripheral bus (0x3F000000–0x3FFFFFFF) mapped Device,
  SP fixup uses detected DRAM_BASE instead of hardcoded 0x40000000
- serial.rs: PL011_BASE_PA_RPI3 = 0x3F201000 (was 0x3F215030), IMSC at offset 0x038
- boot/mod.rs: early_uart_base() RPi3 = 0x3F201000; board detection BEFORE first pl011_puts;
  kernel_phys_range/parse_memory_regions use dram_base() from DTB (works for both boards)
- board.rs: detect_from_dtb_ptr() parses raw FDT for "bcm2837"/"raspberrypi" compatible

**Previous working commits (2026-07-03):**
- `462cc567` — aarch64: implement PL011 UART RX interrupt for interactive shell
- `806511eba` — aarch64: fix RPi3 timer regression — restore CNTV in set_next_timer_rpi3()
- `a5c9f5c8c` — aarch64: fix GIC timer IRQs — set GICC_CTLR ACK_CTL bit (bit 2)
- `7d7d857ed` — aarch64: implement exception table for fallible user-memory access

**Fixed in this session (2026-06-23) — Timer IRQ fixes:**

### QEMU virt — GIC timer fix (`ostd/src/arch/aarch64/gic.rs`)
- **Root cause**: QEMU 6.2 `arm_gic.c` treats all GICC MMIO accesses as "Secure" when
  `has-security-extensions=false` (the default for `-machine virt`).  In Secure view,
  `GICC_CTLR` bit 2 = `ACK_CTL`; without it, `gic_get_current_pending_irq()` returns 1022
  for every Group 1 (NS) interrupt instead of the real IRQ ID.
- Timer PPI 30 is Group 1 (IGROUPR0 bit 30 = 1), so every `GICC_IAR` read returned 1022
  → interrupt never EOI'd → infinite re-entry loop.
- **Fix**: write `0b111` (`EN_GRP0 | EN_GRP1 | ACK_CTL`) to `GICC_CTLR` in both
  `init_on_bsp()` and `init_on_ap()` (was `0b11`).
- Also switched from virtual timer (CNTV, IRQ 27) to physical NS timer (CNTP, IRQ 30) for
  QEMU, because QEMU 6.x EL2 blocks CNTV delivery to NS-EL1 via CNTHCTL_EL2.
- **Verified**: QEMU boots cleanly, `IAR=0x1e` (IRQ 30), task scheduler ticks, `/ #` prompt.

### RPi3 — CNTV timer path (`ostd/src/arch/aarch64/timer/mod.rs`)
- **Root cause**: The QEMU CNTP fix inadvertently broke RPi3 because `set_next_timer_arch()`
  was made global. RPi3's BCM2836 only routes `CNTVIRQ` (bit 3 of CORE0_TIMER_INT_CONTROL);
  arming CNTP fires `CNTPNSIRQ` (bit 1), which is never enabled → no timer IRQ.
- **Fix**: split into `set_next_timer_arch()` (CNTP for QEMU) and `set_next_timer_rpi3()`
  (CNTV for RPi3). `SET_NEXT_TIMER_FN` is set to `set_next_timer_rpi3` during `init()` when
  `BoardType::RaspberryPi3` is detected.
- **Verified on real RPi3 3B hardware** (serial capture 2026-06-23):
  - U-Boot TFTP loads kernel8.img (3.4 MiB) + initramfs.cpio (4.5 MiB)
  - `[kernel] rootfs is ready`, Asterinas logo, `[drv] setup uart irq=57`
  - `[task-loop] top-of-loop` fires continuously (CNTV ticks, watchdog kicked)
  - `/ #` shell prompt appears on serial
  - No watchdog reset during entire boot

**Fixed in this session (2026-07-23) — RPi3 interactive shell (PL011 RX):**
- Implemented **PL011 UART RX interrupt** (`ostd/src/arch/aarch64/serial.rs`)
- Implemented **BCM2835 peripheral IRQ routing** (`ostd/src/arch/aarch64/bcm2836_irq.rs`)
- Wired up **`Pl011Console::register_callback()`** (`kernel/src/driver/mod.rs`)
- **RESULT: Interactive shell works on RPi3.** Typing `echo hello` → `hello` echoed.

**Fixed in this session (2026-07-23) — exception table for fallible user access:**
- `__memcpy_fallible`, `__memset_fallible`, `__atomic_load_fallible`, `__atomic_cmpxchg_fallible`
  implemented in AArch64 assembly with per-instruction `.ex_table` entries.
- `sync_exception_current` checks `.ex_table` for EL1 DABTs from user-space addresses.
- Without this, `write()` with an invalid user buf caused EL1 DABT hang.

**Current boot status (2026-07-03):**

| Platform | Boot | Timer | Shell prompt | Interactive input | Notes |
|----------|------|-------|-------------|------------------|-------|
| QEMU virt (cortex-a72) | ✅ | ✅ CNTP IRQ 30 | ✅ `/ #` | ❌ stdin multiplexing issue | RX interrupt implemented |
| RPi3 3B (hardware) | ✅ | ✅ CNTV IRQ 16 | ✅ `/ #` | ✅ works | first-boot PSCI reset |

**Remaining diagnostic probes in tree (to remove):**
- `kernel/src/fs/rootfs.rs`: `[unpack]` probes, `[rootfs]` entries
- `kernel/src/fs/mod.rs`: `[fs]` enter/done
- `kernel/src/lib.rs`: `[kt1]` milestones in `init_in_first_kthread`
- `kernel/src/thread/task.rs`: `[task-loop]` top-of-loop
- `kernel/src/driver/mod.rs`: `[drv]` uart irq setup
- `kernel/src/syscall/*.rs`: `[sc]`/`[sr]`/`[sw]`/`[spoll]`/`[sip]` probes
- `ostd/src/arch/aarch64/trap/mod.rs`: exception handler loops instead of panic+reset
- `ostd/src/panic.rs`: panic handler loops instead of reset on AArch64

**Known remaining bugs:**
1. QEMU stdin multiplexing — `-nographic` multiplexes serial+monitor on stdio; in headless/PTY environments, commands sent to stdin don't reach the guest serial console. RX interrupt is implemented but cannot be automated-test verified in QEMU. Works on real RPi3 hardware.
2. `/bin/busybox ls /bin` → SIGSEGV (stat/lstat ABI issue, not TLS)
3. First-boot PSCI reset on RPi3 (D-cache coherency gap during CPIO extraction / ELF load)
4. P2.1 SMP deferred (QEMU PSCI CPU_ON returns success but AP never starts)
5. P2.3 reboot syscall (169) not implemented

**Next suggested tasks:**
1. Remove remaining `[task-loop]`, `[sc]`/`[sr]`/`[sw]`, `[drv]`, `[kt1]`, `[sip]` probes
2. Fix `ppoll` stdin behavior so QEMU interactive session works (or wire up QEMU serial PTY)
3. Investigate RPi3 first-boot PSCI reset (D-cache flush after CPIO extraction?)
4. Fix `/bin/busybox ls /bin` SIGSEGV (stat/lstat struct layout mismatch?)
5. Implement SMP AP startup via PSCI CPU_ON with spin-table or PSCI patching

See `.github/agent_state/HANDOFF_2026-06-23.md` for full technical handoff notes.

## AArch64 Shell Boot Command
```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -smp 1 -m 512M \
  -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  -dtb test/nix/aarch64-virt.dtb \
  -device loader,file=test/build/virt-init.dtb,addr=0x47000000,force-raw=on \
  -device loader,file=test/build/aarch64-shell-initramfs.cpio.gz,addr=0x48000000,force-raw=on \
  -append "console=ttyAMA0" -nographic -display none
```

## AArch64 Initramfs Build
```bash
docker run --rm -v $(pwd):/root/asterinas -w /tmp asterinas/aarch64-dev:atest bash -c '
# Install tools, build busybox+glibc via Nix, create cpio + patched DTB
# See test/build/{init.cpio.gz,virt-init.dtb} for pre-built artifacts
'
```

## CI Runner Subagent

Use "ci-runner" keyword to invoke this subagent for running AArch64 CI tests.

**Runnable locally:**
- Lint: `./tools/format_all.sh --check`
- Compile: `cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64`
- Boot Test:
  ```bash
  qemu-system-aarch64 -machine virt -cpu cortex-a72 -smp 1 -m 512M \
    -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
    -dtb test/nix/aarch64-virt.dtb \
    -device loader,file=test/build/virt-init.dtb,addr=0x47000000,force-raw=on \
    -device loader,file=test/build/aarch64-shell-initramfs.cpio.gz,addr=0x48000000,force-raw=on \
    -append "console=ttyAMA0" -nographic -display none
  ```

**Usage:** Just type "run ci tests for aarch64" or "run lint and boot test" in chat.

## AArch64 CI Workflow

The CI workflow is defined in `.github/workflows/test_aarch64.yml`.

**Triggers:**
- Push to `aarch64_support` branch (automatic)
- Manual trigger via `workflow_dispatch` for milestone verification

**Test Jobs:**
- `basic-test`: lint, compile, ktest
- `integration-test`: boot (multiboot2, handover64), general-test, syscall (LTP)
- `microvm-test`: boot test with microvm scheme

**Running CI manually (GitHub CLI):**
```bash
gh workflow run test_aarch64.yml -f milestone=P5.2
```

**Running CI locally (manual commands):**
On WSL2, Docker cannot use KVM. Run QEMU tests directly on host:

```bash
# 1. Lint
./tools/format_all.sh --check

# 2. Compile
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64"

# 3. Boot test (run QEMU directly, not in Docker)
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -smp 1 -m 512M \
  -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  -dtb test/nix/aarch64-virt.dtb \
  -device loader,file=test/build/virt-init.dtb,addr=0x47000000,force-raw=on \
  -device loader,file=test/build/aarch64-shell-initramfs.cpio.gz,addr=0x48000000,force-raw=on \
  -append "console=ttyAMA0" -nographic -display none
```

**Local test feasibility:**

| Test | Can run locally? | Notes |
|------|-----------------|-------|
| lint | ✅ Yes | `./tools/format_all.sh --check` |
| compile | ✅ Yes | Docker with cargo osdk build |
| boot | ✅ Yes | QEMU direct with existing initramfs |
| ktest | ❌ No | Requires Nix daemon for building initramfs |
| general-test | ❌ No | Requires Nix daemon for building initramfs |
| syscall (LTP) | ❌ No | Requires Nix daemon for building initramfs |
| microvm | ❌ No | Needs SCHEME=microvm support |

**Note:** Test infrastructure (ktest, general-test, syscall) requires Nix with a running daemon for building initramfs. Setting up Nix daemon in Docker requires root privileges and proper user/group configuration. These tests are verified via GitHub Actions CI where Nix is pre-configured.

**Current status:** The `asterinas/aarch64-dev:latest` Docker image does not include Nix. Building initramfs locally requires additional Docker configuration that is complex. For now, use GitHub Actions CI for full test verification.

**Note for local testing (WSL2):**
- Docker on WSL2 cannot use KVM, so QEMU tests must be run directly on host
- Build and lint work in Docker
- For QEMU tests, run `qemu-system-aarch64` directly or use TCG mode

**CI Test Status:** See `.github/agent_state/AARCH64_PLAN.md#ci-test-status`

## Build Commands

```bash
# Local build
cd kernel && CARGO_TARGET_DIR=/path/to/new_dir cargo build --release --target aarch64-unknown-none-softfloat

# Docker build (recommended)
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64

# Find binary at: target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf
```

## RPi3 Quick Reference

### Serial Monitor (one line)
```bash
stty -F /dev/ttyUSB0 115200 raw -echo && cat /dev/ttyUSB0
```

### Build boot.scr from boot.cmd (requires u-boot-tools)
```bash
# Install once: sudo apt-get install u-boot-tools
mkimage -A arm64 -O linux -T script -C none -a 0 -e 0 \
  -n "Asterinas RPi3 boot" \
  -d test/rpi3/boot.cmd \
  /mnt/d/pi_sd/boot.scr
# IMPORTANT: do NOT use Python to generate boot.scr — CRC/padding differs from mkimage
```

### Build kernel + deploy to SD card (TFTP root)
```bash
# 1. Rebuild OSDK if linker script templates changed (run once after template edits):
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest bash -c '
  cd /root/asterinas/osdk
  cargo install --path . --root /root/asterinas/.docker-cargo'

# 2. Delete stale base crate if OSDK was rebuilt (OSDK reuses base crate unless deleted):
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  rm -rf /root/asterinas/target/osdk/aster-nix-run-base/

# 3. Build kernel ELF:
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest bash -c '
  PATH=/root/asterinas/.docker-cargo/bin:$PATH
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64'

# 4. Convert ELF → raw binary and deploy:
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest bash -c '
  OBJCOPY=~/.rustup/toolchains/nightly-2025-02-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objcopy
  $OBJCOPY -O binary \
    /root/asterinas/target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
    /root/asterinas/target/osdk/aster-nix/kernel8_raw.bin'
cp target/osdk/aster-nix/kernel8_raw.bin /mnt/d/pi_sd/kernel8.img
# Verify ARM64 magic (should be "41 52 4d 64"):
od -An -tx1 -j0x38 -N4 /mnt/d/pi_sd/kernel8.img
```

### Copy initramfs to TFTP root
```bash
cp test/build/initramfs.cpio.gz /mnt/d/pi_sd/initramfs.cpio.gz
```

### SD card layout (D:\pi_sd\ = TFTP root AND FAT boot partition)
- `kernel8.img` — raw AArch64 binary (from llvm-objcopy -O binary)
- `initramfs.cpio.gz` — initramfs (loaded by U-Boot via TFTP, passed to kernel via DTB)
- `boot.scr` — U-Boot script (generated by mkimage, NOT Python)
- `config.txt` — RPi firmware config (arm_64bit=1, kernel=u-boot-rpi3b.bin)

### RPi3 boot flow
U-Boot (from SD) → TFTP downloads kernel8.img + initramfs.cpio.gz → booti sets DTB initrd props → kernel reads initramfs from DTB linux,initrd-start/end

## RPi3 TFTP Deploy (after build)

`booti` requires a raw binary, not ELF. Use `llvm-objcopy` from the Rust toolchain:

```bash
# Convert ELF → raw binary (ARM64 Image magic at offset 0x38 is preserved)
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest bash -c '
  OBJCOPY=~/.rustup/toolchains/nightly-2025-02-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objcopy
  $OBJCOPY -O binary \
    /root/asterinas/target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
    /root/asterinas/target/osdk/aster-nix/kernel8_raw.bin
  # verify: should show "41 52 4d 64"
  od -An -tx1 -j0x38 -N4 /root/asterinas/target/osdk/aster-nix/kernel8_raw.bin
'

# Deploy to TFTP server root (= FAT SD card at /mnt/d/pi_sd/).
# U-Boot downloads kernel8.img via TFTP on every boot from this directory.
cp target/osdk/aster-nix/kernel8_raw.bin /mnt/d/pi_sd/kernel8.img
```

**Why:** The OSDK ELF entry point is at VMA `0x40080000`. When U-Boot loads it at PA `0x80000`, the ELF header (not the `.boot` section) sits at `0x80000`. The ARM64 Image magic (`0x644d5241`) is in the `.boot` section header at offset `0x38` from the section start, but `booti` checks `load_addr + 0x38`. A raw binary strips the ELF header so the `.boot` section content starts at byte 0, putting the magic at the correct offset `0x38`.

## QEMU AArch64 Boot with DTB and Initramfs

```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -smp 1 -m 512M \
  -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  -dtb test/nix/aarch64-virt.dtb \
  -device loader,file=test/build/virt-init.dtb,addr=0x47000000,force-raw=on \
  -device loader,file=test/build/aarch64-shell-initramfs.cpio.gz,addr=0x48000000,force-raw=on \
  -append "console=ttyAMA0" -nographic -display none
```

## Known AArch64 Issues and Fixes

- **CRITICAL: Use AArch64 initramfs, not x86-64**: `test/build/init.cpio.gz` is the
  x86-64 CI initramfs — using it on AArch64 QEMU causes a silent hang in `spawn_init_process`
  because ELF header machine check fails (Machine::X86_64 ≠ Machine::AArch64). Always use
  `test/build/aarch64-shell-initramfs.cpio.gz` + `test/build/virt-init.dtb` for AArch64 QEMU tests.

- **PL011 UART RX interrupt** (`ostd/src/arch/aarch64/serial.rs`, `kernel/src/driver/mod.rs`):
  Added RX MMIO registers (DR, FR, IM) and `has_data()`, `receive()`, `init_rx_irq()` functions.
  UART IRQ 33 handler reads bytes from UART DR and calls console callback. Works on RPi3 hardware.
  QEMU has stdin multiplexing issue unrelated to kernel code (commands don't reach serial in headless mode).

- **PL011 UART virtual address**: After MMU is enabled, use kernel linear map VA (`0xffff_8000_0900_0000`) not raw PA (`0x0900_0000`). See `ostd/src/arch/aarch64/serial.rs`.
- **`arm-gic` version**: Pin to `=0.6.0` in `ostd/Cargo.toml` to avoid compatibility issues.
- **TLB flush**: `tlbi vmalle1` with IRQs enabled hangs on QEMU 6.2 TCG; needs `daifset #3` mask around it.
- **Initramfs hang (CRITICAL BUG)**: In `ostd/src/mm/frame/meta.rs:547`, physical address was cast directly to pointer without `paddr_to_vaddr()`. Always use `paddr_to_vaddr(meta_pages)` when converting PA to pointer on AArch64.
- **Panic stack trace**: Disabled on AArch64 (`_Unwind_Backtrace` may hang); basic panic handler works.
- **VirtIO MMIO probe**: QEMU `virt` machine exposes VirtIO via MMIO at addresses like `0xa000000`, NOT PCI. Implemented `aarch64_probe()` in `kernel/comps/virtio/src/transport/mmio/bus/mod.rs` to parse device tree.
- **AArch64 `__memcpy_fallible` was not fallible (RPi3 hang fix)**: The original implementation
  used `core::ptr::copy` which causes an unrecoverable EL1 DABT when a user-space pointer is not
  mapped.  This caused `write`/`writev` syscalls with invalid buffers to hang the kernel rather
  than return `EFAULT`.  Fixed by:
  1. Implementing `__memcpy_fallible`, `__memset_fallible`, `__atomic_load_fallible`, and
     `__atomic_cmpxchg_fallible` in AArch64 assembly (`ostd/src/arch/aarch64/mm/memcpy_fallible.S`)
     with per-instruction exception-table entries (`.pushsection .ex_table, "a"`) so faults jump
     to a fixup epilogue that returns a non-zero error code.
  2. Adding a `.ex_table` section to both AArch64 linker templates (`osdk/src/base_crate/aarch64.ld.template`
     and `aarch64-rpi3.ld.template`) to collect and expose the table.
  3. Adding `ostd/src/arch/aarch64/ex_table.rs` (mirrors `ostd/src/arch/x86/ex_table.rs`) to
     search the table at runtime.
  4. Patching `sync_exception_current` in `ostd/src/arch/aarch64/trap/mod.rs` to call
     `find_recovery_inst_addr(f.elr_el1)` when the VMAR page-fault handler fails for a user-space
     address, and redirect ELR_EL1 to the fixup instead of hanging.
  5. Rebuilding `cargo-osdk` with `OSDK_LOCAL_DEV=1 cargo install --path osdk --locked` so the
     embedded linker templates are updated (normal `cargo install` without `--locked` picks a
     newer `libflate` that does not compile on nightly-2025-02-01).

## AArch64 Code Locations

- Boot: `ostd/src/arch/aarch64/boot/`
- CPU: `ostd/src/arch/aarch64/cpu/`
- MM: `ostd/src/arch/aarch64/mm/`
- Trap: `ostd/src/arch/aarch64/trap/`
- Timer: `ostd/src/arch/aarch64/timer/`
- Task: `ostd/src/arch/aarch64/task/`
- Serial: `ostd/src/arch/aarch64/serial.rs`
- IRQ: `ostd/src/arch/aarch64/irq.rs`
- Exception table: `ostd/src/arch/aarch64/ex_table.rs`
- Kernel syscall: `kernel/src/syscall/`
- Driver: `kernel/src/driver/mod.rs`
- VirtIO MMIO: `kernel/comps/virtio/src/transport/mmio/`

## Bring-up Phases

QEMU `virt` -> ARM simulator -> real hardware (Raspberry Pi 3B+)
