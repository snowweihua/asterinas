# Resume Prompt

Use this in a new chat to restore context:

```text
I am resuming AArch64 support work on the asterinas kernel (branch: aarch64_support).

Please read these files to get full context before doing anything:
1. .github/agent_state/PRE_VACATION_CHECKPOINT.md  — where we were before vacation
2. .github/agent_state/CURRENT_CHECKPOINT.md       — phase status and what's working
3. .github/agent_state/NEXT_STEP.md                — the exact next problem to solve (TLB flush hang)

Summary of current state:
- Build & run works: `OSDK_LOCAL_DEV=1 cargo osdk run --scheme aarch64 --target-arch aarch64`
- User space shell is working, timer IRQs work, basic syscalls work
- Blocked on: P2.3 page fault handoff — `tlbi vmalle1` hangs in QEMU 6.2 with DAIF.I=0
- Next action: follow the fix strategy in NEXT_STEP.md

Keep state updates in .github/agent_state/ as you go.
```
