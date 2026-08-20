# Tasks: AArch64 RPi3 v1.1

**Baseline**: `aarch64_v1.0.1` — verified RPi3 shell boot, working shell command execution, and 10/10 clean power-cycle boots. QEMU now also boots to shell and `ls` works after the relative timer fix.

**Execution order**: Phase A → Phase B → Phase C. Do not enable SMP until the single-core baseline remains reproducible.

**Environment**: WSL2 development; Windows TFTP root `D:/pi_sd/`, mapped as `/mnt/d/pi_sd/`; `/srv/tftp` is not used. Physical SD-card files must be copied manually by the user.

**Verification**: Build/deploy through the Build MCP; power off → clear serial → power on → wait 140–150 seconds → read serial until empty. Record TFTP failures separately from kernel failures.

---

## Baseline — v1.0.0 / v1.0.1 evidence

- [x] B001 Build and deploy the current AArch64 kernel and initramfs.
- [x] B002 Boot the RPi3 to an interactive `~ #` shell.
- [x] B003 Verify `echo shell-ok` returns output and a prompt.
- [x] B004 Verify the dynamic busybox/glibc runtime no longer reports `getrandom` or `freeaddrinfo` lookup errors.
- [x] B005 Add a top-level `/init` to the Nix-built dynamic initramfs.
- [x] B006 Add glibc runtime library links under `/usr/lib` in the initramfs.
- [x] B007 Reject failed kernel/initramfs TFTP transfers instead of booting stale `${filesize}` data.
- [x] B008 Disable RPi3 virtual timer initialization and use `SimpleOnce` for the AArch64 timer singleton.
- [x] B009 Run 10 full power cycles; all 10 reached `~ #`.
- [x] B010 Create tag `aarch64_v1.0.0`.
- [x] B011 Fix QEMU init hang by using RPi3-style relative timer (`cntp_tval_el0`) instead of absolute compare timer (`cntp_cval_el0`). QEMU now boots to shell and `ls` works.
- [x] B012 Create tag `aarch64_v1.0.1`.

---

## Phase A — Known issues and regression gates

**Exit criteria**: known issue status is accurate; stat behavior is validated; the RPi3 periodic timer is enabled and validated; `reboot -f` is hardware-tested; plain PID1 `reboot` is deprioritized until A0 is complete; UART scope is decided; QEMU regression passes; this task file has no superseded v1.0 tasks.

### A0 — RPi3 periodic timer (urgent, do first)

- [x] A001 Re-enable `timer::init()` on RPi3 and wire the BCM2836 non-secure physical timer (CNTPNSIRQ, IRQ 30) through the existing `IrqLine`/`bcm2836_irq` path.
- [x] A002 Validate `sleep`, `nanosleep`, and scheduler `Waiter` timeouts on RPi3 without the CNTPCT busy-wait workaround.
- [x] A003 Re-run 10 power-cycle boot baseline after the timer is enabled and confirm no intermittent hang. (10/10 cold-power `BOOT_OK` boots reached, no hang.)

### A1 — Documentation and issue reconciliation

- [x] A101 Compare `tasks.md`, `plan.md`, `research.md`, `quickstart.md`, and `known-issues.md` with `SESSION_CONTEXT.md` and remove stale v1.0 claims.
- [x] A102 Remove obsolete `/srv/tftp`, `kernel8.img` payload, compressed-initramfs, and temporary debug-marker instructions.
- [x] A103 Document the current RPi3 timer policy: the non-secure physical timer (CNTPNSIRQ, IRQ 30) is used for a 1000Hz tick; the virtual timer is not enabled.
- [x] A104 Add a repeatable v1.1 build/deploy/smoke-test checklist and evidence format.

### A2 — AArch64 stat ABI

- [x] A201 Compare `kernel/src/syscall/stat.rs` with the Linux/glibc AArch64 `struct stat` offsets, alignment, and total size.
- [x] A202 Add compile-time layout assertions for the AArch64 `Stat` layout.
- [x] A203 Validate `stat`, `lstat`, `fstat`, symlink metadata, and `ls /bin` on RPi3; `ls /bin` and `ls -la /` now return to the prompt on hardware.
- [x] A204 Implement the smallest ABI correction: add the glibc reserved tail and preserve the verified field offsets; cross-target build passes.
- [ ] A205 Record the final ABI decision and remove the stale SIGSEGV workaround note after hardware `ls`/metadata validation.
- [x] A206 Wire the existing AArch64 exception-table fallible memory-copy helpers; the previous path used raw `core::ptr::copy` for user writes, which could hang on a user-page fault. Hardware revalidation confirmed with `ls /bin` and `ls -la /`.

### A3 — Reboot syscall

- [x] A301 Trace `kernel/src/syscall/reboot.rs`, `kernel/src/syscall/mod.rs`, and the AArch64 syscall table; reconcile the stale research/task claims.
- [x] A302 Validate Linux reboot magic values, command values, error paths, PSCI reset, and PSCI poweroff behavior. Forced reboot now uses the registered AArch64 syscall and RPi3 SMC path; plain PID1-mediated `reboot` still needs follow-up.
- [x] A303 Test `busybox reboot -f` on RPi3 and verify PSCI reset followed by a successful shell boot; plain PID1-mediated `reboot` remains as future init/user-space work.
- [ ] A304 Test the QEMU PSCI reboot path and update quickstart/research documentation.

### A4 — UART scope

- [ ] A401 Document the working mini-UART console as the v1.0 baseline.
- [ ] A402 Decide whether PL011 migration is required for v1.1; defer it if mini-UART satisfies current requirements.
- [ ] A403 If migration is selected, isolate it behind the existing board/serial abstraction and test TX, RX, IRQ acknowledgement, and interactive shell behavior.

### A5 — QEMU and code gates

- [x] A501 Run the documented AArch64 virt boot with the correct AArch64 initramfs and verify `/ #` — QEMU now boots to shell and `ls` works after using RPi3-style relative timer (`cntp_tval_el0`) instead of absolute compare timer (`cntp_cval_el0`).
- [ ] A502 Run affected crate checks/tests and `./tools/format_all.sh --check` (attempted: `format_all.sh --check` reported many pre-existing rustfmt diffs across the tree).
- [ ] A503 Establish a pre-merge gate covering QEMU boot, RPi3 shell smoke tests, TFTP validation, and no stale deployment paths.

---

## Phase B — RPi3 SMP secondary-core bring-up

**Exit criteria**: one AP reaches an online marker; then at least two and ultimately all four cores come online reliably; scheduler and synchronization smoke tests pass; single-core fallback remains available.

### B1 — Audit the current protocol

- [ ] B101 Trace `ostd/src/arch/aarch64/boot/smp_rpi3.rs`, `ap_boot.S`, generic `boot/smp.rs`, and `bcm2836_irq.rs`.
- [ ] B102 Resolve AP destination/address inconsistencies, including the `AP_BOOT_DEST_PA` comments/constants, link/load placement, AP info region, and the AP stub's fixed DRAM-base conversion.
- [ ] B103 Confirm DTB `cpu-release-addr` values and BCM2836 ARM-local peripheral offsets on real hardware.
- [ ] B104 Verify that AP boot paths do not use exclusive atomics or `spin::Once` before single-core-safe initialization is complete.

### B2 — Observable AP protocol

- [ ] B201 Add temporary BSP/AP markers for stub copy, cache clean, info publication, spin-table write, mailbox/event wakeup, AP entry, MMU enable, stack/TLS setup, and online publication.
- [ ] B202 Add bounded timeouts and explicit failure markers instead of indefinite polling.
- [ ] B203 Validate cache maintenance, barriers, identity mappings, and page-table visibility for the stub/info/spin-table regions.
- [ ] B204 Remove temporary probes after the failure boundary is established.

### B3 — AP initialization

- [ ] B301 Replace stale hard-coded physical assumptions with linker/runtime-derived values where required.
- [ ] B302 Initialize AP page tables, stack, CPU-local base, interrupt state, and scheduler state in a verified order.
- [ ] B303 Implement or validate BCM2836 mailbox/event wakeup semantics for each secondary core.
- [ ] B304 Keep the v1.0 single-core path selectable until AP boot is stable.

### B4 — SMP validation

- [ ] B401 Bring up one secondary core and verify an online marker.
- [ ] B402 Bring up all four cores and verify `/proc/cpuinfo` or an equivalent online-CPU report.
- [ ] B403 Run scheduler, mutex, page-table, fork/exec, TLS, and interrupt smoke tests with SMP enabled.
- [ ] B404 Run repeated power cycles with SMP enabled and compare against the single-core baseline.

---

## Phase C — Stress testing, cleanup, and release hygiene

### C1 — Boot and userspace stress

- [ ] C101 Expand boot testing beyond 10 cycles with cycle-level outcome records: TFTP, kernel markers, prompt, symbol errors, exceptions, and reset status.
- [ ] C102 Run shell loops for `echo`, `true`, `ls`, `stat`, `lstat`, `cat /proc/interrupts`, file creation/removal, and symlink operations.
- [ ] C103 Stress `fork`, `vfork`, `clone`, `wait4`, `execve`, signals, and TLS-sensitive dynamic programs.
- [ ] C104 Repeat reboot tests after A3 is complete.
- [ ] C105 Classify network/TFTP failures separately from kernel and userspace failures.

### C2 — Performance and capacity

- [ ] C201 Measure power-on-to-prompt and TFTP transfer times over a representative sample.
- [ ] C202 Track initramfs size and identify safe closure reductions after correctness is stable.
- [ ] C203 Revisit compressed initramfs only after a reliable AArch64 decompression path is proven; do not use the known broken U-Boot inflate path.

### C3 — Architecture and shared-code cleanup

- [ ] C301 Remove stale debug strings, empty conditionals, contradictory comments, and obsolete task references.
- [ ] C302 Consolidate RPi3 board detection, timer policy, IRQ setup, and single-core helpers behind clear board-specific abstractions.
- [ ] C303 Audit direct `spin::Once`, exclusive atomic, 16-byte aggregate-return, and hard-coded physical-address use in AArch64 paths.
- [ ] C304 Keep architecture-specific unsafe code under `ostd/src/arch/aarch64/` and document safety assumptions.
- [ ] C305 Avoid non-architecture changes unless a regression test demonstrates the need.
- [ ] C306 Run formatting, affected crate checks, QEMU regression, RPi3 smoke tests, and stress tests.

### C4 — Documentation and release

- [ ] C401 Update `known-issues.md`, `research.md`, `plan.md`, `quickstart.md`, and this task file with post-v1.0 evidence.
- [ ] C402 Document the Windows TFTP root, manual SD-card copy requirement, power/serial procedure, and failure classification.
- [ ] C403 Define v1.1 acceptance criteria and create a release tag only after the selected Phase A/B/C gates pass.

---

## Dependencies

1. Complete A1 documentation reconciliation and A2/A3 source audits first.
2. Complete Phase A before enabling SMP; preserve a reproducible single-core boot configuration.
3. Execute Phase B incrementally: one AP, then all APs, then SMP stress.
4. Run Phase C stress after each major Phase A/B change; perform broad cleanup after behavior is stable.
5. Mark a task complete only when the corresponding QEMU or RPi3 evidence exists.

## Constraints

- RPi3 is currently single-core and must avoid Cortex-A53 exclusive operations in affected paths.
- Cortex-A53 16-byte return hazards can move under instrumentation; use minimal probes and remove them after diagnosis.
- RPi3 virtual timer initialization remains disabled until separately validated.
- Runtime files are deployed to `/mnt/d/pi_sd/`; `/srv/tftp` is not used.
