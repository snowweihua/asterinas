# RPi3 Hardware Bring-Up Context

This file records durable findings and the current active investigation. Probe chronology, superseded hypotheses, and reverted experiments are intentionally omitted.

## Current Status

- Target: Raspberry Pi 3 Model B, AArch64, single-core runtime.
- The physical board boots the init process to an interactive `/ #` prompt.
- The verified boot baseline completes:
  - metadata mapping and kernel page-table activation;
  - discovery and sorting of all 12 static component records;
  - bootstrap component initialization;
  - `init_in_first_process`, device-node creation, and `ramfs.mknod`;
  - startup of `/bin/sh`;
  - short and long serial commands, including `ls` and `echo`;
  - `update_cpu_time`, `softirq`, and `loadavg` timer callbacks without hangs during 30+ second idle waits.
- The latest diagnostic image also boots to `/ #`, but `exec /bin/busybox echo hi` still hangs after the execve diagnostic path. Therefore normal shell interactivity is the baseline, not a confirmed successful execve result.
- The last physical test left the board powered on. The latest verified image was built, converted, deployed, power-cycled, and captured over serial.

## Durable RPi3 Constraints

### Single-Core and Exclusive-Atomic Constraint

The RPi3 bring-up environment faults on Cortex-A53 exclusive operations (`ldxr`, `ldaxr`, `stxr`, `ldaxrb`, and related CAS loops), even when the target memory is otherwise readable and writable. The board is deliberately treated as single-core during bring-up.

Confirmed affected areas include page-table node locks, frame reference-count transitions, allocator free-size updates, heap-slab reference-count initialization, runtime `Once`, Arc weak-count updates, component inventory registration, and softirq enabled-mask updates.

RPi3-specific paths use plain load/store or boot-safe single-core helpers at runtime-confirmed boundaries. Generic atomic behavior remains unchanged for other targets.

### Cortex-A53 16-Byte Return Hazard

The RPi3 Cortex-A53 can corrupt x30 when a function returns a 16-byte aggregate in registers, including `Result<Frame<M>>`, `Result<UniqueFrame<M>>`, `Option<Paddr>`, and `(FreeChunk, FreeChunk)`. The failure is alignment/layout-sensitive, so adding or removing instrumentation can move the apparent hang.

The allocator mitigations use single-register pointer/physical-address returns with null or `NO_PADDR` sentinels. They include the free-chunk split path, frame-cache and pool allocation, global frame allocation, `MetaSlot::get_from_unused`, and the `Frame::init_unused`/`UniqueFrame::init_unused` helpers. New RPi3 debugging should preserve this constraint and avoid treating a moved boundary as a new root cause without a hardware toggle.

## Confirmed Fixes

### Page Tables, Frames, and DRAM

- The new kernel page table must not copy the boot slot-0 descriptor into metadata root index `0x1c0`. Leaving that root entry empty lets the cursor allocate a managed subtree from the reserved bootstrap page-table pool.
- Boot page-table frames are reserved from a managed pool and initialized with the correct `PageTablePageMeta` level. The bootstrap pool uses plain mutable state while the system is single-core.
- Intrusive buddy-list links created before managed page-table activation can retain physical metadata pointers. `LinkedList::take_current()` restores the frame from its live metadata pointer, and `MetaSlot::frame_paddr()` distinguishes retained low physical pointers from mapped high-virtual metadata pointers.
- Hardware confirmed local buddy allocation, balancing, frame-cache refills, buddy splitting, right-child insertion, and `pools::alloc()` completion.
- `board::dram_base()` is read once and cached. Re-parsing the early-boot FDT after the init process activates its own `TTBR0` page table used to access an invalid low VA and hang in `Frame::init_unused`; the cached base reaches the shell prompt.

### RPi3-Safe Initialization and Components

- `ostd::sync::Once` dispatches to `SimpleOnce` on RPi3. Confirmed migrations cover task handlers, scheduler state, user page-fault handling, RNG, bootstrap component singletons, and bottom-half handlers.
- `arc_new_cyclic()` and `weak_clone()` provide the corresponding single-core Arc paths for systree construction.
- AArch64 component records are emitted into a retained `.component_registry` linker section and enumerated directly because the normal `.init_array` path is not executed. The OSDK run-base cache includes generated linker scripts so linker changes invalidate stale generated bases.
- Generated component names and paths remain borrowed static strings instead of being copied into owned `String` values during bootstrap. Hardware confirms metadata parsing, registry matching, sorting, and component dispatch through block, console, input, PCI, softirq, and systree.

### Logger Backend

The original logger failure was an EL1 synchronous abort in `spin::once::Once::try_call_once_slow`; the OSTD logger injection path now uses boot `SimpleOnce`. The mechanism-matched fix is in place, but a clean post-fix image has not reached `[cmp.logger] init`, so full logger-toggle verification is still unconfirmed.

### Mini-UART RX and Console

- VideoCore firmware can overwrite the ARM peripheral interrupt controller's `ENABLE_IRQS_1`, clearing the AUX mini-UART RX enable bit and making the shell non-interactive while TX still works.
- `reenable_miniuart_irq()` restores the AUX bit. Interrupt acknowledgement also disables and clears the spurious `SYSTEM_TIMER1` pending interrupt that otherwise causes an IRQ storm.
- `init_rx_irq()` reinitializes the mini-UART while preserving the U-Boot baud rate, clears FIFOs, resets `MCR`/`LCR`, and routes GPIO 14/15 to mini-UART alt5 with pull-up/down disabled.
- The timer-polling RX fallback was removed; the AUX IRQ drains the RX FIFO directly. `serial::send()` uses the mini-UART TX-space status and avoids blocking in the IRQ echo path when local IRQs are disabled.
- User-side follow-up edits simplified the UART IRQ handler to call `poll_uart_input()` directly and changed console writers to send one byte at a time through `serial::send()`. Those edits have not yet received a post-change physical verification.

### Scheduler Timer Callbacks

`loadavg` originally deadlocked in the timer interrupt because `ClassScheduler::nr_queued_and_running()` locked every per-CPU runqueue, including a runqueue possibly held by the interrupted context. The fix reads only the local runqueue with `try_lock()` and returns `(0, 0)` when it is contended. All timer callbacks now run together while the shell remains responsive.

### AArch64 Signal Return

- AArch64 glibc relies on the kernel's `__kernel_rt_sigreturn` support instead of supplying `sa_restorer`. The kernel now maps an executable per-process trampoline at `MAX_USERSPACE_VADDR - PAGE_SIZE` containing `mov x8, #__NR_rt_sigreturn` followed by `svc #0`.
- Signal setup uses that trampoline as `x30`/`lr`, unless a user `sa_restorer` is provided. AArch64 context support saves and restores `x30`, and `rt_sigaction` without `SA_RESTORER` is accepted.
- Hardware no longer shows `SA_RESTORER` warnings or the earlier user-space exceptions (`0x92000047`, `0x82000007`).

## Active Investigation: execve Return Boundary

- In `kernel/src/syscall/execve.rs`, `renew_vm_and_map(ctx)` was replaced with `ctx.process.vm().clear_and_map()` to avoid the old VM replacement/drop path.
- The physical command `exec /bin/busybox echo hi` reaches `[A]` through `[K]`, then `[1]`, `[E]`, `[2]`, `[3]`, `[X]`, `[4]`, and `[7]`.
- The caller-side `[5]` marker in `sys_execve` is not observed (`sys_execveat` has an analogous `[6]` marker). Current evidence places the failure after the `[7]` marker and before observable caller completion. The remaining boundary includes the IRQ-guard drop, cleanup/destructor paths, the returned `Result<()>`, and the caller-side marker; ELF loading and `clear_and_map()` are not the current suspects.
- `force_marker()` uses direct RPi3 mini-UART high-half MMIO and is temporary diagnostic code. The current probe also disables local IRQs and intentionally forgets several locals. These experiments are not a fix and must be removed or isolated before a final implementation is committed.
- No durable execve fix has been established. The next test should isolate one post-`[7]` boundary at a time, then follow the required build/deploy/power-cycle/serial verification sequence before claiming progress.

## Operational Notes

- Physical verification sequence:
  1. Build the AArch64 OSDK image.
  2. Convert the ELF to `/tmp/asterina.img`.
  3. Deploy it to `/mnt/d/pi_sd/asterina.img`.
  4. Power off the board.
  5. Clear the serial buffer.
  6. Power on the board.
  7. Wait about 60 seconds, then read serial repeatedly until empty.
- Runtime evidence is authoritative. Do not promote a suspected boundary to a root cause without a reproducible observation and a toggle or equivalent causal proof.
- Temporary UART probes, `early_print`/`pl011_puts`-style probes, and `.debug-journal.md` must not remain in the working tree after an investigation is complete.
