# Roleflow Skill

Implements a sequential role-based development workflow for RPi3 hardware bringup.

## Workflow Chain

```
Planner → Developer → Builder → Verifier → (loop to Planner)
```

## Role Definitions

| Role | Next | Responsibilities |
|------|------|-----------------|
| planner | developer | Read SESSION_CONTEXT.md, research, analyze, decide next action |
| developer | builder | Implement code only; never build/deploy/commit |
| builder | verifier | Compile and deploy to TFTP root |
| verifier | planner | Boot board, capture UART, analyze serial log |

## Decision Outcomes

- **IMPLEMENT**: Pass to Developer
- **RESEARCH**: Planner continues analysis
- **COMMIT**: Save progress, continue loop
- **STOP**: End workflow

## Usage

```
/roleflow         - Start workflow for current task
/roleflow <task>  - Start workflow for specific task (e.g., T030)
/roleflow next    - Advance to next role
/roleflow status  - Show current state
/roleflow abort   - Cancel workflow
```

## Files

- `.roleflow/{planner,developer,builder,verifier}.md` - Role templates
- `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md` - Task context (Planner updates)
- `.github/agent_state/roleflow_state.json` - Workflow state

## Context Isolation

Each role ONLY reads its own template file. Previous role outputs are NOT carried over unless explicitly passed via task summary.
