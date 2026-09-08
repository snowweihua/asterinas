# RPi3 Hardware Bring-Up Context

This file records the current active state. All historical investigations and probe chronology have been removed.

## Current Status

- **SMP bringup: 4/4 CPUs online and executing Rust** — all APs drop from EL2→EL1, enable MMU, enter `ap_early_entry`, call `report_online_and_hw_cpu_id`, and halt cleanly via `halt_cpu()` loop.
- **Heap allocator: working on all 4 CPUs** — SpinLock uses proper `compare_exchange` for multi-core mutual exclusion; AP idle threads spawn successfully.
- **Init process: FIXED — boots to shell prompt on RPi3 hardware** — kernel-mode data abort at `BuddySet::alloc_chunk` (FAR=0x2bfc000) was caused by physical MetaSlot pointers in buddy free lists becoming unmapped after TTBR0 switched to user page table.
- **Shell stdin/stdout: FIXED — shell is fully interactive** — ENOENT panic at `create_init_task` line 141 was caused by trying to open `/dev/console` before `device::init_in_first_process` created it (initramfs `/dev/` is empty; `/dev/console` is created by device init AFTER `spawn_init_process`). Fix: removed manual stdin/stdout/stderr setup from `create_init_task` — `init_in_first_process` handles it when init task first runs.
- **Shebang scripts: FIXED** (commit `76c5cad1`) — script path is now appended to the interpreter argv; RPi3 `/init` runs and `ls` works.
- **QEMU smoke test: PASSES on `raspi3b`** (commit `5bb16ff7`) — prompt, echo, and `ls` all pass. Two root causes fixed: (a) ARM-local base was `0x3F000000`, must be `0x40000000` (timer enable, IRQ acknowledge, and spin-table writes went nowhere); (b) PSCI SMC probe hung with no EL3 firmware — now gated on `psci_usable()` (EL3 present + 19.2MHz CNTFRQ).
- **QEMU SMP: 4/4 CPUs online** — same image takes the spin-table path there (no EL3 firmware for PSCI). Three QEMU-specific incompatibilities fixed: (a) slots are absolute `0xD8+mpidr*8` per QEMU `hw/arm/raspi.c` (not ARM-local offsets); (b) AP stub falls back to the global info array when `x0==0` (QEMU ROM stub zeroes regs; PSCI passes context in `x0`); (c) AP stub drops EL3→EL2 first (QEMU starts secondaries at EL3, TF-A starts them at EL2). Hardware keeps the proven PSCI path unchanged.
- **UART RX: FIXED via timer-tick polling fallback** — `poll_uart_input()` also runs on every timer tick, so serial input works even if the UART IRQ is lost; QEMU echo verified.
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
- **Buddy allocator MetaSlot pointer conversion** (commit `d9dde9c7`): During bootstrap, `get_slot()` returns physical MetaSlot pointers stored in linked list `front`/`back`/`next`/`prev` pointers. After `activate_kernel_page_table()` + `IN_BOOTSTRAP_CONTEXT=false`, these physical pointers become unmapped when TTBR0 switches to user page table. Fix: `LinkedList::convert_pointers()` traverses free lists via identity-mapped physical pointers and converts all `NonNull<Link<M>>` pointers to `FRAME_METADATA_RANGE` virtual addresses using `meta_slot_paddr_to_vaddr()`.
- **Shell stdin/stdout ENOENT fix** (commit `381202a8`): `create_init_task` tried to open `/dev/console` during init task creation, but `/dev/console` doesn't exist yet (initramfs `/dev/` is empty; device init runs AFTER `spawn_init_process`). Fix: removed manual stdin/stdout/stderr setup from `create_init_task`. The `init_in_first_process` function properly sets up stdin/stdout/stderr when the init task first runs (after device init creates `/dev/console`).
- **Shebang script path** (commit `76c5cad1`): `program_loader` now appends the script's absolute path to the interpreter argv — fixes shebang scripts silently ignored (RPi3 `/init` never ran, no shell).
- **EL3/CNTFRQ-gated PSCI probe** (commit `5bb16ff7`): `is_psci_available()` and the PSCI diagnostic table are skipped unless `psci_usable()` (EL3 implemented + CNTFRQ 19.2MHz) — fixes QEMU `raspi3b` boot hang in the first `smc` with no EL3 firmware.
- **ARM-local base address** (commit `5bb16ff7`): `LOCAL_IC_BASE_PA` and `ARM_LOCAL_PA` corrected from `0x3F000000` (BCM2835 window) to `0x40000000` (QA7 ARM-local) — timer IRQ enable, IRQ-source acknowledge, and spin-table writes now reach real registers; QEMU serial RX and timer tick work.
- **UART RX polling fallback** (commit `5bb16ff7`): `timer::register_callback_on_cpu(poll_uart_input)` drains the PL011 FIFO every tick in addition to the IRQ handler.
- **Heap bootstrap pointer conversion**: heap slab lists and per-CPU slot caches never got the frame allocator's physical-to-virtual conversion, leaving stale pointers that faulted under fork churn; now converted at ostd init (global + BSP cache) and AP entry (per-CPU), with selective (idempotent, mix-safe) walks.
- **Unique slab-list IDs**: all `LinkedList`s shared ID 1, so `dealloc` removed full-list slabs via the wrong list object and corrupted both chains; IDs are now unique per list.
- **Atomic RPi3 refcount/mutex paths**: `inc_count` load/store and `Mutex` load/store replaced with atomic `fetch_add`/`swap`, matching std semantics on SMP.

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
- QEMU `raspi3b` smoke test passes (prompt + echo + `ls`); the pre-commit hook runs it automatically — do not skip with `SKIP_SMOKE_TEST=1` unless the failure is proven unrelated.
- QEMU `raspi3b` boots 4/4 SMP via the spin-table slots it polls; use physical RPi3 hardware to validate the PSCI path and real timing.
- Commit format: `<area>: <what changed> — <why/result>`.
