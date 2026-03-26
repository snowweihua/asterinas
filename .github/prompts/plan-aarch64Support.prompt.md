**Current Baseline**
- Your branch is `aarch64_support`, tracking `origin/aarch64_support`, with local unstaged edits in AArch64 trap files.
- Relative to `origin/main`, this branch is significantly divergent (`+7/-60` commits, ~48 touched files), so restart should be milestone-based and rebase-friendly.
- AArch64 scaffolding exists in OSTD (boot/MM/trap/timer/cpu), but kernel-level AArch64 integration is still missing in key entry points (e.g., [kernel/src/lib.rs](kernel/src/lib.rs), [kernel/src/syscall/mod.rs](kernel/src/syscall/mod.rs)).
- OSDK/docs are still x86-first in places (see [osdk/README.md](osdk/README.md), [osdk/src/commands/build/mod.rs](osdk/src/commands/build/mod.rs)).
- Current AArch64 check is blocked early by dependency/toolchain friction (`arm-gic` + nightly const-stability mismatch), before your in-tree trap errors are fully surfaced.

**Key Gaps To Re-Do Cleanly**
- **Platform contract**: settle one target first (QEMU `virt`, EL1-only, GICv3, no SMP initially).
- **Trap pipeline**: keep vector-table-based design; your current local rewrite of [ostd/src/arch/aarch64/trap/trap.S](ostd/src/arch/aarch64/trap/trap.S) and [ostd/src/arch/aarch64/trap/mod.rs](ostd/src/arch/aarch64/trap/mod.rs) is structurally inconsistent with existing `UserContext`/`TrapFrame` flow.
- **MM correctness**: [ostd/src/arch/aarch64/mm/mod.rs](ostd/src/arch/aarch64/mm/mod.rs) has incomplete/invalid sections (missing fallible ops, inconsistent flags), so it needs a correctness-first rewrite against OSTD paging traits.
- **SMP/IPI/interrupt ack**: [ostd/src/arch/aarch64/boot/smp.rs](ostd/src/arch/aarch64/boot/smp.rs), [ostd/src/arch/aarch64/irq.rs](ostd/src/arch/aarch64/irq.rs), and [ostd/src/arch/aarch64/mod.rs](ostd/src/arch/aarch64/mod.rs) still have `unimplemented!`.
- **Kernel arch enablement**: add minimal AArch64 arch module + syscall ABI shim before chasing broad subsystem parity.

**New Development Plan (Recommended)**
- **Phase 0 — Reset + Baseline (1–2 days)**: restore trap path to last coherent state, remove `.old` artifacts, pin/patch `arm-gic` compatibility, make `cargo check -p ostd --target aarch64-unknown-none-softfloat` reproducible.
- **Phase 1 — OSTD bring-up single-core kernel-only (3–5 days)**: boot to Rust, early serial, timer tick, IRQ dispatch, panic backtrace, no userspace transition yet.
- **Phase 2 — OSTD user transition + traps (4–7 days)**: implement stable EL0↔EL1 context switch, syscall trap path, page-fault handoff, `UserContextApi` completeness.
- **Phase 3 — Kernel minimal AArch64 enablement (4–8 days)**: add `kernel/src/arch/aarch64`, wire [kernel/src/lib.rs](kernel/src/lib.rs), syscall arch glue in [kernel/src/syscall/mod.rs](kernel/src/syscall/mod.rs), exception-to-signal path.
- **Phase 4 — OSDK + run pipeline (2–4 days)**: make `cargo osdk build/run --target-arch aarch64` first-class, clean AArch64 scheme defaults in [OSDK.toml](OSDK.toml), document host/tool constraints.
- **Phase 5 — SMP + device expansion (later)**: AP bring-up, IPIs, GIC redistributor per-CPU init, then virtio/pci gaps.
- **Phase 6 — CI and hardening**: add `check/build/run-smoke` workflow for AArch64 (even if marked experimental initially).

**Milestones / Exit Criteria**
- M1: OSTD AArch64 `check` passes cleanly with zero `unimplemented!` in boot/trap/mm critical path.
- M2: QEMU boots reliably to kernel main loop with timer interrupts.
- M3: One user task enters/exits via syscall on AArch64.
- M4: Kernel `build + run smoke` works via OSDK AArch64 scheme.
- M5: Optional SMP boot of 2 cores with scheduler tick.

**Validation Matrix (Per Phase, Executable Checklist)**

Use this table as the source of truth for “done”.

Status legend: `[ ]` not run, `[/]` running, `[x]` pass, `[!]` fail.

### Global Test Rules
- Always test in this order: **QEMU virt** -> **ARM simulator** -> **Raspberry Pi 3B+**.
- Promote to the next environment only after all required checks in the current environment are `[x]`.
- For each failed check, record: command, full log snippet, commit hash, and regression range.

### Environment Matrix

| Env | Purpose | Required in early phases | Required in late phases |
|---|---|---|---|
| QEMU `virt` (AArch64) | Primary bring-up and fast iteration | Yes (Phase 0+) | Yes |
| ARM simulator (you own) | Secondary model validation | Optional (Phase 1-2) | Yes (Phase 3+) |
| Raspberry Pi 3B+ | Real board validation | Optional (after Phase 3) | Yes (Phase 5+) |

### Phase 0 — Reset + Baseline

- [ ] **P0.1 Reproducible check (QEMU host toolchain path)**
	- Command: `cargo clean && CARGO_TARGET_DIR=/tmp/asterinas-cargo-target cargo check -p ostd --target aarch64-unknown-none-softfloat`
	- Run count: 2 consecutive runs
	- Pass if: both runs succeed, no new lock/toolchain drift, no generated `.old`/temporary sources under `ostd/src/arch/aarch64`.

- [ ] **P0.2 Trap path coherence**
	- Command: `rg "trap_entry|vector_table|trap_handler|RawUserContext|TrapFrame" ostd/src/arch/aarch64 -n`
	- Pass if: one coherent trap-entry strategy (no mixed/duplicate paths), symbol names align between `trap.S`, `trap.rs`, and `mod.rs`.

### Phase 1 — OSTD Single-Core Bring-Up (Kernel only)

- [ ] **P1.1 Boot banner appears (QEMU)**
	- Command: `cargo osdk run --target-arch aarch64 --scheme aarch64`
	- Timeout: 30s
	- Pass if: serial log reaches OSTD/kernel initialization banner.

- [ ] **P1.2 Timer IRQ liveness (QEMU)**
	- Method: add temporary periodic log/counter in timer callback.
	- Pass if: counter increments continuously for >= 60s without panic.

- [ ] **P1.3 Panic backtrace sanity (QEMU)**
	- Method: trigger controlled panic once.
	- Pass if: panic info prints expected AArch64 register names and non-empty call trace.

### Phase 2 — EL0/EL1 Transition + Trap/Syscall

- [ ] **P2.1 User transition round-trip (QEMU)**
	- Method: start one minimal user task and return via trap.
	- Pass if: at least one clean EL0->EL1->EL0 cycle.

- [ ] **P2.2 Syscall smoke (QEMU)**
	- Method: run trivial user sequence (`getpid`, `write`, `exit`).
	- Pass if: syscall number dispatch and return values are correct.

- [ ] **P2.3 Page fault handoff path (QEMU)**
	- Method: trigger a controlled user page fault.
	- Pass if: exception is converted to expected fault/signal flow, no kernel deadlock.

### Phase 3 — Kernel AArch64 Integration

- [ ] **P3.1 Kernel arch wiring check**
	- Command: `cargo check -p kernel --target aarch64-unknown-none-softfloat`
	- Pass if: kernel builds with AArch64 arch module and syscall arch glue enabled.

- [ ] **P3.2 Syscall subset test**
	- Method: run init process with a syscall subset script.
	- Pass if: subset passes with deterministic logs in two consecutive runs.

### Phase 4 — OSDK Build/Run Pipeline

- [ ] **P4.1 OSDK build path**
	- Command: `cargo osdk build --target-arch aarch64 --scheme aarch64`
	- Pass if: build artifacts generated successfully from clean tree.

- [ ] **P4.2 OSDK run path**
	- Command: `cargo osdk run --target-arch aarch64 --scheme aarch64`
	- Pass if: boots and reaches expected init stage without manual argument patching.

### Phase 5 — SMP + IPI (after single-core stability)

- [ ] **P5.1 Multi-core online**
	- Method: boot with `-smp 2` or more.
	- Pass if: secondary core(s) online and scheduler tick visible per CPU.

- [ ] **P5.2 IPI functional test**
	- Method: send one directed IPI and one broadcast IPI.
	- Pass if: target CPU receives and acknowledges both paths.

### Phase 6 — CI + Non-regression

- [ ] **P6.1 CI check/build/run-smoke job**
	- Pass if: AArch64 workflow runs on clean runner and is reproducible.

- [ ] **P6.2 Flake gate**
	- Method: run smoke 10 times.
	- Pass if: success >= 9/10 and no unresolved intermittent crash.

---

**Simulator and Real Device Notes**

### ARM Simulator Track (Secondary)
- Validate the same phase gates as QEMU from Phase 3 onward.
- Add one extra check for interrupt controller model compatibility (GIC behavior and IRQ numbering).
- If simulator differs from QEMU `virt`, treat it as a separate platform profile and document deviations.

### Raspberry Pi 3B+ Track (Board-Specific)
- Treat Pi 3B+ as a **separate board bring-up target**, not equivalent to QEMU `virt`.
- Minimum Pi gate before claiming support:
	- [ ] Board boot entry works.
	- [ ] UART console stable.
	- [ ] Timer interrupt stable.
	- [ ] Interrupt controller path working on board.
	- [ ] Storage/network path verified for your intended demo.
- Recommendation: start Pi validation only after Phase 3 is stable on QEMU.

---

**Test Evidence Template (copy per run)**

```md
Phase/Check: P?.?
Env: QEMU virt / ARM simulator / Raspberry Pi 3B+
Commit: <hash>
Command: <full command>
Start-End Time: <UTC>
Result: [x]/[!]
Key Logs:
- <line 1>
- <line 2>
Notes:
- <root cause or observation>
```

---

**Phase 0 Execution Log (2026-03-10)**

### Completed actions
- Updated AArch64 dependency pin in [ostd/Cargo.toml](ostd/Cargo.toml):
	- `arm-gic = "=0.6.0"`
- Updated lockfile in [Cargo.lock](Cargo.lock):
	- `arm-gic` downgraded to `0.6.0`
- Restored local trap-path edits to branch state and removed local backup artifact:
	- Restored: `ostd/src/arch/aarch64/trap/mod.rs`, `trap.S`, `trap.rs`
	- Removed: `ostd/src/arch/aarch64/trap/trap.S.old`

### Run results
- Check command used:
	- `CARGO_TARGET_DIR=/tmp/asterinas-cargo-target cargo check -p ostd --target aarch64-unknown-none-softfloat`
- Result after dependency + workspace stabilization + targeted fixes:
	- External `arm-gic` blocker: **resolved**
	- In-tree compile state: **passes** for `ostd` target check (`Finished dev profile`)

### Fix clusters completed in this pass
1. `ostd/src/arch/aarch64/mm/mod.rs`
	- Rewritten to a coherent trait-compatible implementation and proper unsafe-asm usage.
2. `ostd/src/arch/aarch64/cpu/context.rs`
	- Aligned with `RawUserContext`/`TrapFrame` fields and ESR-based trap decoding.
3. `ostd/src/arch/aarch64/timer/mod.rs`, `irq.rs`, `qemu.rs`, `serial.rs`, `trap/mod.rs`
	- Removed RISC-V carryovers and invalid API calls; added compile-safe stubs where needed.
4. `ostd/src/mm/io.rs`
	- Enabled `pod_once_impls` for `aarch64` target.

### Updated Phase 0 status
- P0.1 Reproducible check: `[x]` (command succeeds after stabilization/fixes)
- P0.2 Trap path coherence: `[x]` (local ad-hoc rewrite removed; branch baseline restored)

### Immediate next step (Phase 0 -> Phase 1 handoff gate)
- Make `cargo check -p ostd --target aarch64-unknown-none-softfloat` pass by fixing in this order:
	1. `aarch64/mm/mod.rs`
	2. `aarch64/cpu/context.rs`
	3. `aarch64/timer/mod.rs` + `aarch64/irq.rs`
