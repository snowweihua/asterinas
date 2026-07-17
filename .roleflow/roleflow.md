# Roleflow Workflow

Sequential role-based development workflow for RPi3 hardware bringup debugging.

## Workflow Chain

```
Planner → Developer → Builder → Verifier → (loop to Planner)
```

## Complete Command Walkthrough

### Start Workflow
```
/roleflow T030
```
or for current task:
```
/roleflow
```

---

### Step 1: PLANNER

```
role_start({role: "planner"})
```
- Reads SESSION_CONTEXT.md
- Analyzes problem
- Decides next action

**Finish with one of:**
```
role_finish({summary: "<action description>", progress: "progress"})      # IMPLEMENT
role_finish({summary: "<reason>", progress: "no_progress"})                # RESEARCH
role_finish({summary: "<description>", progress: "finished"})              # STOP
```
Note: COMMIT is done by updating SESSION_CONTEXT.md, then progress

---

### Step 2: DEVELOPER

```
role_start({role: "developer"})
```
- Receives Planner's decision in contextSummary
- Implements the code change
- Does NOT build, deploy, or commit

**Finish:**
```
role_finish({summary: "<file changed and what was done>", progress: "progress"})
```

---

### Step 3: BUILDER

```
role_start({role: "builder"})
```
- Runs build commands from builder.md
- Deploys to TFTP root

**Finish with one of:**
```
role_finish({summary: "Build SUCCESS, deployed to /mnt/d/pi_sd/", progress: "progress"})
role_finish({summary: "BUILD_FAILED: <exact error>", progress: "build_failed"})
```
Note: build_failed routes to Developer to fix the error

---

### Step 4: VERIFIER

```
role_start({role: "verifier"})
```
1. **YOU say**: "Please power on/reset the RPi3 board now"
2. Runs serial capture: `cat /dev/ttyUSB0 > /tmp/serial_log.txt`
3. Waits up to 60 seconds
4. Reads log

**If capture fails (no output or U-Boot stuck):**
```
role_finish({summary: "NO_OUTPUT or U-Boot stuck", progress: "user_action_needed"})
```
→ Workflow PAUSES, you fix board issue, then resume with:
```
/roleflow continue
```

**If capture success:**
```
role_finish({summary: "<full raw serial log>", progress: "progress"})
```

---

### Loop Back to Planner

After Verifier, workflow returns to Planner to analyze the serial log.

---

## Progress Values

| Value | Meaning | Next Role |
|-------|---------|-----------|
| `progress` | Made progress, continue | Next in chain |
| `no_progress` | No progress, need research | Planner |
| `finished` | Task complete, stop | (none) |
| `build_failed` | Build error, need fix | Developer |
| `user_action_needed` | Hardware issue, pause | Verifier (after resume) |

## Utility Commands

```
workflow_status()              # Check current state
/roleflow continue             # Resume after user_action_needed pause
```

## State Files

- Task context: `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md`
- Workflow state: `.github/agent_state/roleflow_state.json`

## Key Rules

1. **Planner** NEVER writes code, builds, or captures serial
2. **Developer** ONLY implements, never build/deploy/commit
3. **Builder** ONLY compiles and deploys
4. **Verifier** ONLY captures serial, passes raw log to Planner
5. **Context isolation**: Each role only reads its own template
