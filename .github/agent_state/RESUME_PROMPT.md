# Resume Prompt

Use this in a new chat to restore context:

```text
I am resuming AArch64 support work on the asterinas kernel (branch: aarch64_support).

Please read these files to get full context before doing anything:
1. .github/agent_state/PRE_VACATION_CHECKPOINT.md  — authoritative last status (2026-03-27)
2. .github/agent_state/CURRENT_CHECKPOINT.md       — detailed phase status and what's working

NOTE: NEXT_STEP.md is STALE (2026-03-25) — the TLB problem it describes was already solved.
      The PRE_VACATION_CHECKPOINT.md is the source of truth.

Summary of current state:
- Build & run works: `OSDK_LOCAL_DEV=1 cargo osdk run --scheme aarch64 --target-arch aarch64`
- User space shell (busybox sh) is working, timer IRQs work, basic syscalls work
- Last commit: 05ea31fd (Remove debug UART probes and print statements from AArch64 boot code)

Next steps (in order):
1. Phase 6 — Add AArch64 CI workflow to .github/workflows/
2. SMP — implement PSCI for bringup_all_aps() in ostd/src/arch/aarch64/boot/smp.rs

Keep state updates in .github/agent_state/ as you go.
```
