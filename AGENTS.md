# Asterinas Agent Instructions

## Project Overview

Asterinas is a Linux-compatible OS kernel written in Rust, using the framekernel architecture. The repository contains:
- `kernel/` - The aster-nix kernel
- `ostd/` - Rust OS framework (OSTD)
- `osdk/` - OSDK toolkit (cargo-osdk CLI)
- `test/` - Test suite (apps, LTP/gvisor syscall tests, benchmarks)

## Key Build Commands

```bash
make build          # Build kernel (requires initramfs + cargo-osdk)
make run            # Run kernel in QEMU
make test           # User-mode tests for non-OSDK crates only
make ktest          # Kernel-mode tests (runs in QEMU)
make check          # clippy + format check + typos
make format         # Format code
make docs           # Build Rust documentation
```

## Critical Distinction: Two Types of Crates

**Non-OSDK crates** can use standard `cargo test/clippy`:
- `ostd/libs/align_ext`, `ostd/libs/id-alloc`, `ostd/libs/ostd-macros`, `ostd/libs/ostd-test`
- `kernel/libs/aster-rights`, `kernel/libs/aster-rights-proc`, `kernel/libs/atomic-integer-wrapper`
- `kernel/libs/cpio-decoder`, `kernel/libs/int-to-c-enum`, `kernel/libs/jhash`, `kernel/libs/keyable-arc`
- `kernel/libs/logo-ascii-art`, `kernel/libs/typeflags`, `kernel/libs/typeflags-util`

**OSDK crates** require `cargo osdk` (installs automatically via `make build`):
- `kernel/`, `ostd/`, `osdk/deps/*`, `kernel/comps/*`, `kernel/libs/aster-util`, `kernel/libs/aster-bigtcp`, `kernel/libs/xarray`

To test a single OSDK crate: `cd <crate-dir> && cargo osdk test`
To test a single non-OSDK crate: `cd <crate-dir> && cargo test`

## Architecture-Specific Build Options

- `OSDK_TARGET_ARCH`: x86_64, aarch64, riscv64, loongarch64 (default: aarch64)
- `RELEASE=1`: Release build
- `RELEASE_LTO=1`: Release with full LTO (slower compile, faster runtime)
- `SMP=N`: Number of CPUs
- `ENABLE_KVM=0`: Disable KVM (for testing)

## Test Execution in CI

CI runs `make check` (lint/format/typos) before compilation tests. Test sequence for general tests:
1. `make check` (lint)
2. `make build FEATURES=all` (compile)
3. `make test` (usermode)
4. `make ktest NETDEV=tap` (kernel mode)

Integration tests run via `make run AUTO_TEST=<type>` with variants for boot, syscall, general, vsock.

## Code Style

- Rust formatting: `rustfmt.toml` at root with `imports_granularity="Crate"` and `group_imports="StdExternalCrate"`
- Workspace lints enabled in all crates (`[lints] workspace = true`)
- Custom typos config in `.typos.toml`

## Development Environment

Requires Docker image `asterinas/asterinas:0.16.0-20250910` or `asterinas/aarch64-dev:latest` with QEMU, KVM, Nix, and OSDK tools pre-installed.

**Critical env variable for local build:**
```bash
export VDSO_LIBRARY_DIR=/path/to/linux/vdso  # Required for make build
```

**Docker build (when local build fails due to permissions):**
```bash
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64
```

## Workspace Configuration

- Root `Cargo.toml` defines workspace members and excludes (comp-sys crates, osdk itself)
- Profile `release-lto` uses `panic = "abort"` (not `unwind`)
- OSTD edition is 2024; kernel edition is 2021

## Agent Session State

Per `.github/copilot-instructions.md`:
- Use `.github/agent_state/` for recovery checkpoints
- Use `target/agent_logs/` for run logs
- `/tmp` is ephemeral only

**Git Workflow:**
- Commits on detached HEAD can be lost when switching branches - always work on a named branch
- Use `git stash` sparingly; commits are safer for persistence
- Reflog (`git reflog`) can recover "lost" commits if needed: `git reflog | grep <keyword>`
- Cherry-pick from reflog to restore: `git cherry-pick <commit-hash>`

## AArch64 Development Notes

### Current Status (as of 2026-04-18)
**Completed:** P0.1, P0.2, P1.1 (debug output), P1.2, P1.3, P2.1, P2.2, P2.3, P3.1, P4.1, P4.2, initramfs hang fix

**Remaining TBD:**
- P1.2 Docker Build Determinism (inconsistent binaries)
- P2.1 SMP/Multicore (PSCI CPU_ON implemented but not tested)
- P2.2 Syscall subset test (P3.2 in plan document)
- P2.3 Poweroff/Restart (not verified)
- P3.1/P3.2 Interrupt handling & memory management improvements
- P4.1 Automated CI tests, P4.2 GDB debugging
- P5.1 Hardware support (Raspberry Pi), P5.2 VirtIO drivers

See `.github/agent_state/AARCH64_PLAN.md` for detailed task list.

### AArch64 Build/Run Commands
```bash
# Build (requires OSDK_LOCAL_DEV for local development)
OSDK_LOCAL_DEV=1 cargo build --target aarch64-unknown-none-softfloat -p aster-nix

# Run in QEMU
cargo osdk run --scheme aarch64 --target-arch aarch64

# Or via make (AArch64 is default)
make build
make run
```

### QEMU AArch64 Boot with DTB and Initramfs
```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -smp 1 -m 512M \
  -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf \
  -dtb test/nix/aarch64-virt.dtb \
  -device loader,file=test/nix/aarch64-virt-patched.dtb,addr=0x47000000,force-raw=on \
  -device loader,file=test/build/initramfs.cpio.gz,addr=0x48000000,force-raw=on \
  -append "console=ttyAMA0" -nographic -display none
```

### Known AArch64 Issues and Fixes
- **PL011 UART virtual address**: After MMU is enabled, use kernel linear map VA (`0xffff_8000_0900_0000`) not raw PA (`0x0900_0000`). See `ostd/src/arch/aarch64/serial.rs`.
- **`arm-gic` version**: Pin to `=0.6.0` in `ostd/Cargo.toml` to avoid compatibility issues.
- **TLB flush**: `tlbi vmalle1` with IRQs enabled hangs on QEMU 6.2 TCG; needs `daifset #3` mask around it.
- **Initramfs hang (CRITICAL BUG)**: In `ostd/src/mm/frame/meta.rs:547`, physical address was cast directly to pointer without `paddr_to_vaddr()`. Always use `paddr_to_vaddr(meta_pages)` when converting PA to pointer on AArch64.
- **Panic stack trace**: Disabled on AArch64 (`_Unwind_Backtrace` may hang); basic panic handler works.

### AArch64 Code Locations
- Boot: `ostd/src/arch/aarch64/boot/`
- CPU: `ostd/src/arch/aarch64/cpu/`
- MM: `ostd/src/arch/aarch64/mm/`
- Trap: `ostd/src/arch/aarch64/trap/`
- Timer: `ostd/src/arch/aarch64/timer/`
- Task: `ostd/src/arch/aarch64/task/`
- Serial: `ostd/src/arch/aarch64/serial.rs`
- IRQ: `ostd/src/arch/aarch64/irq.rs`
- Kernel syscall: `kernel/src/syscall/`

### Architecture Bring-up Phases
See `.github/agent_state/AARCH64_PLAN.md` for full prioritized task list.

Validation order: QEMU `virt` -> ARM simulator -> real hardware (Raspberry Pi 3B+).
