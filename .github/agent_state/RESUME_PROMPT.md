# Resume Prompt

Use this in a new chat to minimize token usage:

```text
Continue from .github/agent_state/CURRENT_CHECKPOINT.md.
Use .github/agent_state/NEXT_STEP.md and .github/agent_state/LATEST_LOG.md.
Only give delta updates and keep new durable state in .github/agent_state/2026-03-13-phase1-handoff-probe.md.
Current intent: execute Phase II (AArch64 GICv2 + IRQ/timer path + kernel init un-gating).
```