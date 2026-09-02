# RPi3 Hardware Bring-Up Context

This file records the current active state. All historical investigations and probe chronology have been removed.

## Current Status

- **SMP bringup: 4/4 CPUs online via trampoline** (committed `3935b624`). BSP detects APs via per-CPU DRAM entry markers and calls `report_online_and_hw_cpu_id` on their behalf. APs execute the assembly stub fully (all DRAM markers appear) but `br x1` to `ap_early_entry` Rust function faults for unknown reason.
- **Known limitation**: APs cannot execute Rust code (branch to `ap_early_entry` fails), so hw_cpu_id values written by trampoline are BSP's MPIDR (incorrect for APs). Map count is correct so BSP reports 4/4.
- **Known issue**: AP UART output (PL011 writes) does not reach serial. DRAM writes work. Debug progression via DRAM markers exclusively.
- Target: Raspberry Pi 3 Model B, AArch64, SMP (4/4 online via trampoline, APs can't run Rust yet).
- The physical board boots the init process to an interactive `~ #` prompt.
- Shell commands (`echo`, `ls`, `uname`, `reboot -f`) work.
- QEMU (raspi3b) also boots to shell prompt.

## Durable RPi3 Constraints

### Single-Core and Exclusive-Atomic Constraint

The RPi3 bring-up environment faults on Cortex-A53 exclusive operations (`ldxr`, `ldaxr`, `stxr`, `ldaxrb`, and related CAS loops). RPi3-specific paths use plain load/store or boot-safe single-core helpers. Generic atomic behavior remains unchanged for other targets.

### Cortex-A53 16-Byte Return Hazard

The RPi3 Cortex-A53 can corrupt x30 when a function returns a 16-byte aggregate in registers, including `Result<Frame<M>>`, `Option<Paddr>`, and `(FreeChunk, FreeChunk)`. The allocator mitigations use single-register pointer/physical-address returns with null or `NO_PADDR` sentinels.

## Confirmed Fixes (Reference)

- Link-derived stub destination (fixed hardcoded overlap with live `.text`)
- ADR fix for `__boot_page_table_pointer` (high-half VA dereference → PA-relative)
- TTBR1 + TCR_EL1 + MAIR_EL1 programming in AP stub (both TTBR0 and TTBR1 must be set)
- IRQ storm fix (CORE_REG_STRIDE 0x400→0x4, `init_on_ap()` masks all ARM-local IRQs)
- BSS-zero removal from AP stub (shared kernel BSS must not be zeroed by APs)
- Trampoline: BSP detects APs via DRAM entry markers and reports online on their behalf
- Timer: BCM2836 non-secure physical timer (CNTPNSIRQ) with relative TVAL, 1000Hz tick
- `execve` TLS: `TPIDR_EL0` for user TLS (not `TPIDR_EL1` which is OSTD CPU-local base)
- PL011 UART on RPi3: GPIO 14/15 ALT0, IRQ 57, IBRD=26 FBRD=3 for ~115200 baud
- Mini-UART RX: `reenable_miniuart_irq()` restores AUX bit after VC firmware overwrites it
- Page table: managed bootstrap pool with correct `PageTablePageMeta` level, slot-0 not copied to metadata root
- `getdents64`: exception-table `memcpy_fallible.S` helpers wired in AArch64 mm

## Operational Notes

- TFTP root: Windows `D:/pi_sd/` = `/mnt/d/pi_sd/`. `/srv/tftp` is NOT the TFTP root.
- Build: single build, same binary for RPi3 hardware and QEMU (uses `aarch64-rpi3` with `cortex-a53`).
- Build MCP in WSL2; serial/power MCP in Windows.
- QEMU: `raspi3b` machine type, `cortex-a53`, 1G, `-nographic` via tmux, DTB at `/mnt/d/pi_sd/bcm2710-rpi-3-b.dtb`.
- Smoke test: `make smoke_test` or `python3 test/rpi3/smoke_test.py` (requires `/tmp/asterina.img` and `test/build/initramfs.cpio`).
- Commit format: `<area>: <what changed> — <why/result>`.
- Skip smoke test: `SKIP_SMOKE_TEST=1 git commit -m "message"`.
