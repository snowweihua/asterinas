---
description: Start a new roleflow workflow with tool-based enforcement
---

# Roleflow Workflow (Tool-Driven)

This workflow is **enforced by tools**, not documentation.

## How It Works

1. Call `workflow_start({task: "T030"})` to begin
2. Each role has **restricted tool set** - only allowed tools work
3. Must call `role_finish()` to advance to next role
4. Tool calls outside allowed set are **BLOCKED**

## Quick Start

```
workflow_start({task: "T030"})
```

## Tool Allowlists (Enforced)

| Role | Allowed Tools |
|------|---------------|
| planner | role_finish, role_start, read, grep, glob, session_read, session_search, bash |
| developer | role_finish, role_start, read, edit, write, glob, grep, bash |
| builder | role_finish, role_start, read, bash |
| verifier | role_finish, role_start, read, bash |

## Commands

- `workflow_start({task: "T030"})` - Start workflow (first role: PLANNER)
- `role_start({role: "planner"})` - Continue/start a role
- `role_finish({summary: "...", progress: "..."})` - Complete role, advance
- `workflow_status()` - Check current state and allowed tools

## Progress Values

| Value | Meaning | Next Role |
|-------|---------|-----------|
| `progress` | Made progress | Next in chain |
| `no_progress` | No progress | Planner |
| `finished` | Task complete | STOP |
| `build_failed` | Build error | Developer |
| `user_action_needed` | Hardware issue | Verifier (after resume) |

## Role Templates

See `.roleflow/{planner,developer,builder,verifier}.md` for role-specific instructions.
