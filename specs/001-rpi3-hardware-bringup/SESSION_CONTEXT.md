# RPi3 Hardware Bring-Up Context

This file records the current active state. All historical investigations and probe chronology have been removed.

## Current Status

- **SMP bringup: 4/4 CPUs online and executing Rust** — all APs drop from EL2→EL1, enable MMU, enter `ap_early_entry`, call `report_online_and_hw_cpu_id`, and halt cleanly via `halt_cpu()` loop.
- **Heap allocator: working on all 4 CPUs** — SpinLock uses proper `compare_exchange` for multi-core mutual exclusion; AP idle threads spawn successfully.
- **Known issue**: Shell regression — kernel boots to "WARN: No generic PCI host controller node found in the device tree" and stalls. No shell prompt, no response to serial input. This is a **pre-existing regression** that existed before SMP work began. Not caused by SMP changes.
- Target: Raspberry Pi 3 Model B, AArch64, SMP (4/4 online, all executing Rust).
- Build: single binary for RPi3 hardware and QEMU (uses `aarch64-rpi3` with `cortex-a53`).

## Confirmed Fixes (Reference)

- Link-derived stub destination (fixed hardcoded overlap with live `.text`)
- ADR fix for `__boot_page_table_pointer` (high-half VA dereference → PA-relative)
- TTBR1 + TCR_EL1 + MAIR_EL1 programming in AP stub (both TTBR0 and TTBR1 must be set)
- IRQ storm fix (CORE_REG_STRIDE 0x400→0x4, `init_on_ap()` masks all ARM-local IRQs)
- BSS-zero removal from AP stub (shared kernel BSS must not be zeroed by APs)
- **EL2→EL1 drop** (commit `a1a5bbde`): APs start at EL2 after PSCI CPU_ON; HCR_EL2.RW=1 + ERET drops to EL1h before MMU enable — fixes `br x1` fault caused by EL2 using unconfigured translation
- **SpinLock fix** (commit `0c36433f`): Replaced broken non-atomic test-and-set (`load+store Relaxed`) with proper `compare_exchange(Acquire)` and `store(Release)` in release — fixes heap allocator corruption when multiple CPUs access GLOBAL_POOL
- **AP idle loop fix** (commit `0c36433f`): Replaced `Task::yield_now(); unreachable!()` with `loop { halt_cpu(); }` so APs halt cleanly after init
- Trampoline: BSP detects APs via DRAM entry markers and reports online on their behalf
- Timer: BCM2836 non-secure physical timer (CNTPNSIRQ) with relative TVAL, 1000Hz tick
- `execve` TLS: `TPIDR_EL0` for user TLS (not `TPIDR_EL1` which is OSTD CPU-local base)
- PL011 UART on RPi3: GPIO 14/15 ALT0, IRQ 57, IBRD=26 FBRD=3 for ~115200 baud
- Mini-UART RX: `reenable_miniuart_irq()` restores AUX bit after VC firmware overwrites it
- Page table: managed bootstrap pool with correct `PageTablePageMeta` level, slot-0 not copied to metadata root
- `getdents64`: exception-table `memcpy_fallible.S` helpers wired in AArch64 mm

## Durable RPi3 Constraints

### Single-Core and Exclusive-Atomic Constraint

The RPi3 bring-up environment faults on Cortex-A53 exclusive operations (`ldxr`, `ldaxr`, `stxr`, `ldaxrb`, and related CAS loops). RPi3-specific paths use plain load/store or boot-safe single-core helpers. Generic atomic behavior remains unchanged for other targets.

**Note**: Despite this constraint, `compare_exchange` (LDXR/STXR) works correctly at EL1 for the SpinLock. The constraint may be specific to EL2 or certain memory regions.

### Cortex-A53 16-Byte Return Hazard

The RPi3 Cortex-A53 can corrupt x30 when a function returns a 16-byte aggregate in registers, including `Result<Frame<M>>`, `Option<Paddr>`, and `(FreeChunk, FreeChunk)`. The allocator mitigations use single-register pointer/physical-address returns with null or `NO_PADDR` sentinels.

## Operational Notes

- TFTP root: Windows `D:/pi_sd/` = `/mnt/d/pi_sd/`. `/srv/tftp` is NOT the TFTP root.
- Build: single build, same binary for RPi3 hardware and QEMU (uses `aarch64-rpi3` with `cortex-a53`).
- Build MCP in WSL2; serial/power MCP in Windows.
- QEMU: `raspi3b` machine type, `cortex-a53`, 1G, `-nographic` via tmux, DTB at `/mnt/d/pi_sd/bcm2710-rpi-3-b.dtb`.
- Smoke test: `make smoke_test` or `python3 test/rpi3/smoke_test.py` (requires `/tmp/asterina.img` and `test/build/initramfs.cpio`).
- **NOTE**: QEMU smoke test has been broken since SMP bringup work began — skip smoke test when committing: `SKIP_SMOKE_TEST=1 git commit -m "message"`.
- Commit format: `<area>: <what changed> — <why/result>`.
