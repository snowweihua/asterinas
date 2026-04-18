# AArch64 Development Agent Instructions

## Development Environment

Requires Docker image `asterinas/aarch64-dev:latest` with QEMU, KVM, Nix, and OSDK tools pre-installed.

**Docker build (when local build fails due to permissions):**
```bash
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64
```

## Agent Session State

**Git Workflow:**
- Always work on a named branch (e.g., `aarch64_support`), not detached HEAD
- Commits on detached HEAD are lost when switching branches
- Use `git stash` sparingly; commits are safer for persistence
- Reflog (`git reflog`) can recover "lost" commits: `git reflog | grep <keyword>`
- Cherry-pick from reflog to restore: `git cherry-pick <commit-hash>`

**Checkpoints:**
- Use `.github/agent_state/` for recovery checkpoints
- Use `target/agent_logs/` for run logs

## Current Status (2026-04-18)

**Completed:** P0.1, P0.2, P1.1 (debug output), P1.2, P1.3, P2.1, P2.2, P2.3, P3.1, P4.1, P4.2, initramfs hang fix

**Remaining TBD:**
- P1.2 Docker Build Determinism (inconsistent binaries)
- P2.1 SMP/Multicore (PSCI CPU_ON implemented but not tested)
- P2.2 Syscall subset test
- P2.3 Poweroff/Restart (not verified)
- P3.1/P3.2 Interrupt handling & memory management improvements
- P4.1 Automated CI tests, P4.2 GDB debugging
- P5.1 Hardware support (Raspberry Pi), P5.2 VirtIO drivers

See `.github/agent_state/AARCH64_PLAN.md` for detailed task list.

## Build Commands

```bash
# Local build
cd kernel && CARGO_TARGET_DIR=/path/to/new_dir cargo build --release --target aarch64-unknown-none-softfloat

# Docker build (recommended)
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64

# Find binary at: target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf
```

## QEMU AArch64 Boot with DTB and Initramfs

```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -smp 1 -m 512M \
  -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  -dtb test/nix/aarch64-virt.dtb \
  -device loader,file=test/nix/aarch64-virt-patched.dtb,addr=0x47000000,force-raw=on \
  -device loader,file=test/build/initramfs.cpio.gz,addr=0x48000000,force-raw=on \
  -append "console=ttyAMA0" -nographic -display none
```

## Known AArch64 Issues and Fixes

- **PL011 UART virtual address**: After MMU is enabled, use kernel linear map VA (`0xffff_8000_0900_0000`) not raw PA (`0x0900_0000`). See `ostd/src/arch/aarch64/serial.rs`.
- **`arm-gic` version**: Pin to `=0.6.0` in `ostd/Cargo.toml` to avoid compatibility issues.
- **TLB flush**: `tlbi vmalle1` with IRQs enabled hangs on QEMU 6.2 TCG; needs `daifset #3` mask around it.
- **Initramfs hang (CRITICAL BUG)**: In `ostd/src/mm/frame/meta.rs:547`, physical address was cast directly to pointer without `paddr_to_vaddr()`. Always use `paddr_to_vaddr(meta_pages)` when converting PA to pointer on AArch64.
- **Panic stack trace**: Disabled on AArch64 (`_Unwind_Backtrace` may hang); basic panic handler works.

## AArch64 Code Locations

- Boot: `ostd/src/arch/aarch64/boot/`
- CPU: `ostd/src/arch/aarch64/cpu/`
- MM: `ostd/src/arch/aarch64/mm/`
- Trap: `ostd/src/arch/aarch64/trap/`
- Timer: `ostd/src/arch/aarch64/timer/`
- Task: `ostd/src/arch/aarch64/task/`
- Serial: `ostd/src/arch/aarch64/serial.rs`
- IRQ: `ostd/src/arch/aarch64/irq.rs`
- Kernel syscall: `kernel/src/syscall/`

## Bring-up Phases

QEMU `virt` -> ARM simulator -> real hardware (Raspberry Pi 3B+)
