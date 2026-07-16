# AArch64 Development Agent Instructions

## last session status refer to specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md
## development important information refer to specs/001-rpi3-hardware-bringup/quickstart.md


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


## Current Session: RPi3 Boot Hang Investigation

**Symptom**: Kernel hangs at `call_ostd_main()` on RPi3 hardware
**Root Cause Found**: `ostd/src/arch/aarch64/cpu/extension.rs` uses `spin::Once` (external crate) which hangs on RPi3
**Fix Applied**: Changed to use `crate::boot::SimpleOnce` instead
**Status**: Fix deployed, awaiting test confirmation





