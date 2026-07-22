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


## Current Session: RPi3 Stack Slot Corruption

**Symptom**: Synchronous Abort at `ret` instruction on RPi3 — saved x30 on stack overwritten with 0xFFFFFFFFC900A8
**Key Canary Finding**: FP register (0x3af4c380) is UNCHANGED at exit. Static canaries NOT corrupted. Only the specific stack slot holding saved x30 is overwritten.
**Current Investigation**: Focus on the `return Err(Error::NoMemory)` path — compiler epilogue may write to the saved-x30 stack slot when dropping `_metadata: M` and building the `Result<Frame<M>, Error>` return value.
**Status**: Stack canary fixed (shared statics). Need to investigate epilogue/drop behavior. Try `#[inline(never)]` or explicit `core::mem::forget(_metadata)`.
**Details**: `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md`





