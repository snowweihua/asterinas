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
- `exec /bin/busybox echo hi` completes successfully on hardware and prints `hi`. The diagnostic trace was subsequently removed and a clean physical run of `exec /bin/busybox echo clean` also printed `clean`.
- The root cause was identified: AArch64 `UserContext::set_tls_pointer()` and `tls_pointer()` used `TPIDR_EL1`, which is also the OSTD CPU-local base register. `execve` reset the user TLS to zero and thereby destroyed CPU-local addressing; the following `preempt_count()` accessed CPU-local storage through address zero. The methods now use `TPIDR_EL0`, which is also preserved by the AArch64 task switch assembly.
- Temporary exec tracing, the public diagnostic `preempt_count()` accessor, and unused UART/IRQ probes have been removed.
- The IRQ workaround around the 16-byte `Result<()>` return was removed: `do_execve` now returns normally and `handle_syscall` no longer performs a manual IRQ enable. A fresh physical run of `exec /bin/busybox echo irqfree` printed `irqfree`, confirming that the TPIDR_EL0 TLS fix alone resolves the hang.
- Clean diagnostic-free hardware output is confirmed. `/bin/busybox sh` replacement has been tested separately but produces no observable `sh -c 'echo ...'` output; this appears to be a distinct shell/job-control/TTY follow-up, not the original execve return hang.
- A fresh test of the shell command `/bin/busybox ls` also hangs after the command is echoed and produces no directory listing or prompt. Temporary syscall/getdents probes were inconclusive because their serial markers were not reliably observable. No ls-path code change is committed; the source tree was restored to the last verified execve-fix state and rebuilt/deployed.
- The generated dynamic initramfs did not contain a top-level `/init`, so successful kernel initialization could still fail to reach the shell prompt. `test/nix/initramfs.nix` now installs `/init` to launch `/bin/busybox sh`; this was committed as `d7bb190a` and the rebuilt dynamic cpio contains `/init`.
- The intermittent boot problem is not yet fully resolved. With the corrected initramfs, some full 140-second cycles still stop after early component/log output; replacing only the init entrypoint with a statically linked busybox did not change that behavior. Do not revert to `asterina_last.img`; the remaining investigation must use the current source and current `asterina.img`.
- A later failed cycle exposed a separate boot-script race: the initramfs TFTP request returned `unable to connect`, but `boot.cmd` did not check its status and reused the kernel's stale `${filesize}`. U-Boot then passed a truncated/invalid initramfs to the kernel. The script now aborts and resets on either kernel or initramfs TFTP failure instead of booting invalid data.
- With a valid transfer, dynamic busybox reported `undefined symbol: getrandom, version GLIBC_2.25`; the matching glibc in the Nix store does export both `getrandom@@GLIBC_2.25` and `freeaddrinfo@@GLIBC_2.17`. The initramfs therefore now exposes the target glibc runtime libraries through `/usr/lib` symlinks, in addition to retaining the Nix store closure. One subsequent valid boot reached `~ #` without the symbol error, but a later valid boot still stopped before the prompt, so kernel-side nondeterminism remains under investigation.
- The AArch64 timer singleton used `spin::Once` even on RPi3, contrary to the RPi3 single-core/exclusive-atomic constraint. It now uses `boot::SimpleOnce`.
- Disabling `timer::init()` entirely on the RPi3 path produced a clean full boot to `~ #` in the next cycle after another valid-transfer stop. This makes the RPi3 timer initialization/interrupt path the current leading cause of the remaining intermittent boot failure; timer callbacks are not required for the initial shell prompt in this bring-up configuration.
- After this change, `/bin/sh` command execution was verified with `echo shell-ok`, which printed `shell-ok` and returned to `~ #`. A subsequent 10-cycle power test reached `~ #` on all 10 cycles, with no TFTP failure, busybox symbol lookup error, or missing-prompt failure.
- The AArch64 reboot syscall was registered at syscall 142 and RPi3 reboot uses PSCI through SMC while QEMU retains HVC. `busybox reboot -f` is verified to reset the board and return to `~ #`; the path is committed and the latest built `asterina.img`/`initramfs.cpio` both pass this test. Plain PID1-mediated `reboot` (non-forced) still does not reset because the busybox `sh` init process does not catch `SIGTERM`/load `ENV` aliases; this is a user-space follow-up rather than a PSCI kernel issue.
- Cross-target glibc headers report the AArch64 `struct stat` size as 128 bytes with `st_rdev@32`, `st_size@48`, `st_blksize@56`, `st_blocks@64`, and timestamps at 72/88/104. `kernel/src/syscall/stat.rs` now includes the reserved tail and compile-time offset assertions; the build passes. Hardware `ls /bin` still stops in the getdents/stat path, so the runtime validation remains open.
- `getdents64` was failing on a user-copy page fault in the AArch64 `memcpy` fallback. The existing exception-table `memcpy_fallible.S` helpers are now wired in `ostd/src/arch/aarch64/mm/mod.rs`. Hardware validation now passes `ls /bin`, `ls -la /`, `stat`/`lstat`/`fstat` symlinks, and `echo`/`true` shell commands.
- RPi3 `timer::init()` is re-enabled: it uses the BCM2836 non-secure physical timer (CNTPNSIRQ) as a 1000Hz periodic tick through `IrqLine` 30. The timer softirq raise path is re-enabled in `kernel/src/time/softirq.rs`, and hardware `sleep 1` now returns to the prompt using the `Waiter`/timeout path.
- A remaining open item is that the dynamic initramfs `busybox` currently fails to resolve `sched_getparam` when invoked as an external command (e.g. `reboot -f`), so a self-contained static initramfs or rootfs update is needed for `reboot` validation.

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

- `do_execve` now returns normally after the new user context is prepared; no exec-specific IRQ guard is leaked and no syscall-side manual IRQ re-enable is performed.
- The earlier physical trace `A0A!E0E#^` localized the hang to the post-`^` `preempt_count()` path. Changing AArch64 user TLS access from `TPIDR_EL1` to `TPIDR_EL0` fixed it: the physical trace became `A0A!E0E#^0$%^0`, followed by the expected `hi` output. A later diagnostic-free run also printed `clean`.
- Root cause: `TPIDR_EL1` is the OSTD CPU-local base register, initialized by boot assembly and required by `CpuLocalCell`. `execve` called `set_tls_pointer(0)`, overwriting that base; the next generic CPU-local load in `preempt_count()` then accessed an invalid address. AArch64 task switching already saves/restores `TPIDR_EL0`, confirming it is the appropriate user TLS register.
- Temporary tracing and unused UART/IRQ probes have been removed. The IRQ workaround has also been removed and physically re-tested with `exec /bin/busybox echo irqfree` producing `irqfree`. The remaining shell-specific follow-up is likely in job-control/TTY initialization.

## Active Investigation: shell command hang (fork/clone)

- The RPi3 page-table lock workaround disables local IRQs only while performing the plain load/store acquisition. The IRQ guard is not stored in `PageTableGuard`; storing it there leaked IRQ-disabled state through cursor aliases and caused `sync_tlb_flush()` to stall or panic.
- The scheduler now keeps all RPi3 tasks on the BSP because the board path reports multiple DTB CPUs but intentionally does not bring up APs. Newly spawned tasks preempt the current task even at equal priority, so fork/vfork children actually run before the parent waits.
- AArch64 `UserContext` now stores TLS per context, activates `TPIDR_EL0` before each user entry, and captures it after returning from user mode. This prevents a forked child’s dynamic linker from using the parent’s stale TLS pointer; the observed `ld.so` fault at `__tls_pre_init_tp` with `FAR=-0x32e` is gone.
- Rebuilt the full aarch64 initramfs from the Nix sources using `asterinas/nix:0.16.1-20250922` (`make -C test OSDK_TARGET_ARCH=aarch64 BENCHMARK=none INITRAMFS_SKIP_GZIP=1`). The 44,041,728-byte cpio was repacked with `/init` and deployed to `/mnt/d/pi_sd/initramfs.cpio`; the full rootfs avoids the minimal busybox/glibc symbol mismatch.
- On the physical RPi3 with the clean kernel and full rootfs, `true`, `ls`, and `/bin/busybox ls` execute successfully and `ls` lists the root directory. Dynamic `execve` also works (`/bin/busybox echo` prints its argument).
- `Mutex::acquire_lock` for RPi3 already disables local IRQs around its plain load+store sequence and has been committed previously.
- The OSDK Docker image was rebuilt earlier and `cargo-osdk` was fixed to build from the current `osdk` source (`Arch::as_str` -> `Arch::to_str`).

## Active Investigation: plain `reboot` vs `reboot -f`

- `busybox reboot -f` resets the RPi3 through the AArch64 `reboot` syscall and PSCI `smc #0` with `x0 = 0x8400_0009`. The board returns to the U-Boot/TFTP boot path and reaches `~ #`.
- Plain `reboot` (busybox without `-f`) is designed to signal `init` and wait for it to shut down the system. The current initramfs runs `busybox sh` as PID1 and does not catch `SIGTERM` or load a profile/`ENV` file, so it returns to the prompt instead of resetting. A wrapper `reboot` script, shell function, and `alias` were all attempted; all of them failed on `reboot -f` when invoked from a shell script, while direct `reboot -f` works.
- The remaining fix is either a self-contained initramfs `reboot` replacement that does not call back into the shell, or implementing proper `SIGTERM`/PID1 shutdown. This is not a kernel PSCI problem.

## Operational Notes

- The Windows TFTP server root is `D:/pi_sd/`, mapped in WSL2 as `/mnt/d/pi_sd/`. `/srv/tftp` is not used for RPi3 deployment; do not copy kernel or initramfs files there.
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
