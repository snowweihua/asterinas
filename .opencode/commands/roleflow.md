---
description: Execute RPi3 hardware bringup task through sequential role workflow: Planner → Developer → Builder → Verifier → (loop)
---

# Roleflow Workflow

You are executing a task through the roleflow sequential workflow for RPi3 hardware bringup.

## Workflow Chain

```
Planner → Developer → Builder → Verifier → (loop to Planner)
```

## Step 1: Identify Task

The user requested: `$ARGUMENTS`

Parse the task ID (e.g., T030) from the input.

## Step 2: Read Context

Read the following files to understand current state:
- `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md` - Current task context
- `.roleflow/workflow.yaml` - Role chain definition
- `.roleflow/planner.md` - Planner role template

## Step 3: Execute Workflow

### Role 1: PLANNER

**Read**:
- SESSION_CONTEXT.md for current problem understanding
- Any relevant research files in `specs/001-rpi3-hardware-bringup/`

**Analyze** and decide next action:
- **IMPLEMENT** → Go to Developer with clear action item
- **RESEARCH** → Continue analysis, then decide
- **COMMIT** → Save progress, continue loop
- **STOP** → End workflow

**NEVER**: Write production code, build, deploy, capture serial logs

### Role 2: DEVELOPER

Receive Planner's decision and implement the code change.

**ONLY**: Implement code based on Planner's decision
**NEVER**: Build, deploy, commit, or do research

When finished → Call `role_finish()` tool to transition

### Role 3: BUILDER

**Compile** the code:
```bash
cd /home/snow/asterinas && make build AARCH64=1
```

**Deploy** to TFTP server root:
```bash
# Copy kernel Image to TFTP root
cp buildboot/kernel/Image /srv/tftp/ || cp target/aarch64-unknown-none-softfloat/release/asterinas /srv/tftp/asterinas.efi
```

When finished → Call `role_finish()` tool to transition

### Role 4: VERIFIER

**Boot and capture serial**:
```bash
# Start serial capture in background
screen -S serial -dm bash -c 'sudo sh -c "cat /dev/ttyUSB0 > /tmp/serial_log.txt"'
# Trigger boot via network
# Wait for boot output
sleep 30
# Stop capture and read log
screen -S serial -X quit 2>/dev/null
cat /tmp/serial_log.txt
```

**Analyze** the serial output:
- If progress made → `role_finish(progress)` → Planner reviews → COMMIT → Developer
- If no progress → `role_finish(no_progress)` → Planner does more RESEARCH
- If task complete → `role_finish(finished)` → STOP

## RPi3 UART Debug Rule

**CRITICAL**: When capturing serial logs or outputting debug info, use `pl011_puts` NOT `early_marker`. The `early_marker` function writes directly to UART without checking LSR bit 6 (TX empty), which can hang if TX buffer is full.

## Workflow State

Store state in `.github/agent_state/roleflow_state.json`:
```json
{
  "currentTask": "T030",
  "currentRole": "planner",
  "cycleCount": 1,
  "lastProgress": "description"
}
```

## Loop Control

After Verifier completes:
- Read the `progress` decision
- If `progress` or `no_progress` → Loop back to Planner
- If `finished` → Output final summary and STOP

## Done When

- STOP decision reached
- All required changes committed
- Final verification report generated
