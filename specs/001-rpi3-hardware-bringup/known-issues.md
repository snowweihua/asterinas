# RPi3 Known Issues

## Issue 1: Enable PL011 UART
**Description**: The bring-up currently uses the mini-UART for the serial console. PL011 is the more capable UART on RPi3 and is the long-term target.
**Impact**: None for v1.1 — mini-UART is functional and satisfies the current shell/serial requirements.
**Status**: Resolved (2026-08-26) — PL011 UART at GPIO 14/15 ALT0 is now the active serial console on RPi3. GPIO alt0 configuration, AUX peripheral management, and IRQ routing are implemented. Shell prompt and interactive commands work on PL011.

## Issue 2: SimpleOnce vs `spin::Once` on RPi3
**Description**: `ostd::sync::Once` dispatches to `boot::SimpleOnce` on RPi3 to avoid Cortex-A53 exclusive-atomic (`LDXR`/`STXR`) issues during single-core bring-up.
**Impact**: Superseded — SMP is live (4/4 online, APs schedule user threads, EL1 atomics verified on hardware). The RPi3 `SimpleOnce` dispatch remains on boot paths (harmless); the AP entry wait moved to `spin::Once` + sevl/wfe/sev event signaling after QEMU APs intermittently parked forever in the LDARB spin despite the set flag.
**Status**: Resolved with updated understanding — exclusives work at EL1 on Normal memory; the remaining exclusive-sensitivity is device/early-boot paths plus the F2 silicon quirk (Issue 4).

## Issue 3: Dynamic `busybox` `reboot -f` reliability
**Description**: Calling the dynamic `busybox` `reboot -f` applet sometimes faulted before reaching the `reboot(2)` syscall, producing intermittent "Segmentation fault" or no reset.
**Impact**: RPi3 reset was unreliable from the shell prompt.
**Status**: Resolved — a static `/bin/reboot` helper is now built into the initramfs; `reboot -f` from the shell reliably triggers PSCI `SYSTEM_RESET`.

## Issue 4: Intermittent exclusive-abort on fresh-heap atomic-inc (HW-only, open)
**Description**: Synchronous external abort (`ESR 0x96000035`) at `ldxr` in heap-alloc + immediate atomic-inc sequences — proven at two sites (`ThreadOptions::build`, `WaitTimeout::wait_until_or_timeout`). Plain stores to the same line succeed immediately before, proving translation valid: not a page-table bug. Same victim VA across boots (deterministic heap layout); HW-only, never QEMU; clusters in the first ~1-2 min post-prompt.
**Impact**: One spawn/sleep operation fails per hit; the system survives (shell works post-HALT). Spawner-ID dump deployed (EL1-SYNC prints CPU=/TASK=). Full dumps: `.github/agent_state/stress-fault-hw*.log`.
**Status**: Open — verdict is silicon/fabric exclusive-handling quirk (same class as tree-documented LDARB aborts). Working model: snoop/exclusive SLVERR on same-line neighbor collision. Mitigation deferred pending rate data; directions are static spawn pool or cache-line isolation (see SESSION_CONTEXT).

## Issue 5: Static pthreads fail at NPTL TLS setup (open, boundary)
**Description**: Static pthread binaries die at startup (`page fault ...ffe0` = TP-32) in a sustained handler loop that never kills the task. vfork, `clone(SIGCHLD)`, sigaction/kill all pass on both targets; `CLONE_SYSVSEM` is explicitly unsupported by the kernel.
**Impact**: No threaded userspace; single-threaded fork/exec/signal programs unaffected. The never-ending fault loop is a kernel robustness gap (should SIGSEGV-kill the task).
**Status**: Open — filed as supported-surface boundary; dynamic TLS programs untested (no loader/libs on target).

## Issue 6: HW single-core via pruned DTB blocked (open, firmware-level)
**Description**: A 1-CPU DTB (cpu@1..3 removed) halts reproducibly after TF-A BL31, before U-Boot — BL31's PSCI topology init requires the full CPU description. Firmware confirmed live-loading the pruned file (size delta matches to the byte).
**Impact**: HW single-core config untestable without an SD DTB the firmware accepts; QEMU single-core (1-CPU DTB) boots to prompt+echo and stands as the single-core evidence. No kernel change warranted.
**Status**: Open — needs SD-card DTB the firmware tolerates, or a kernel `maxcpus`-style override (neither exists today).

## Issue 7: Minimal procfs gaps (open, cosmetic)
**Description**: `/proc/interrupts` absent; `ps` runs but enumerates nothing. Shell, file, affinity, sleep, and signal functionality all verified working.
**Impact**: Observability only; no functional impact.
**Status**: Open — documents the procfs floor; AP/tick proof done via probes instead.

## Issue 8: QEMU-only nested-spawn SEGV flake (open, needs re-characterization)
**Description**: Nested/multi-fork dynamic exec intermittently SEGVs on QEMU raspi3b; hardware auto-test spawns cleanly. Predates the heap fixes; never re-tested on fixed images.
**Impact**: QEMU-only test noise; no HW impact observed.
**Status**: Open — re-characterize on current images under C102/C103 follow-up.
*Last updated: 2026-09-09*
