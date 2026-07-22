# AArch64 Development Agent Instructions

## last session status refer to specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md
## development important information refer to specs/001-rpi3-hardware-bringup/quickstart.md

## Current work flow (may change based on task)
**Research and analysis -> code change -> Build and Deploy -> stop and ask user to power cycle RPi3B ->start serial capture and read log -> If have new progress, commit code and do /checkpoint (save session status) -> continue next research and analysis

## Git rules

### Time to commit
**When new progress, commit it immediately, then do next debugging

### "Commit on Progress" — Non-Negotiable

**After any meaningful change**, commit immediately with a descriptive message:
- Code change that fixes or changes behavior
- Document update with new understanding

**Before ANY risky operation** (checkout/reset/stash):
```bash
git add -A && git commit -m "WIP: <description>"
```

### Commit Message Format
```
<area>: <what changed> — <why/result>
```
Example: `aarch64/cpu: replace spin::Once with SimpleOnce — fixes RPi3 boot hang at enable_cpu_features`


## Current Session: Generic Result Return Crash at `ret` on AArch64 RPi3

**Root Cause**: `alloc_frame_with<M>() -> Result<Frame<M>, Error>` crashes at `ret` when returning ANY value on AArch64 RPi3. Not a compiler bug, not boot PT. Spin loop works (avoids return).
**Fixes applied this session**: boot PT slots 1-3 zeroed on RPi3, `new_kernel_page_table()` copies boot PT entries, linear+meta mapping skipped on AArch64, `KERNEL_PAGE_TABLE` → `SimpleOnce`.
**Blocker**: `PageTableNode::alloc()` still calls `alloc_frame_with()` → spin loop. Need to provide a working Frame allocation that returns `Ok` instead of `Err`.
**MCP tools**: serial_mcp_server (port 8910), deploy_mcp_server (port 8911) — ready for next session.
**Details**: `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md`





