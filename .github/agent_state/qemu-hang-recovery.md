# QEMU Hang Investigation - Recovery Note

## Date: 2026-08-24

## Issue
QEMU hangs after banner print. Shell prompt never appears.

## What Works
- RPi3 boots correctly with same code and initramfs
- Timer initialization works (seen in output)
- All kernel components initialize
- Banner prints correctly

## What's Been Tried

### 1. rseq Syscall Implementation
- Created `kernel/src/syscall/rseq.rs` stub
- Added `mod rseq` to `kernel/src/syscall/mod.rs`
- Added `SYS_RSEQ = 293` to `kernel/src/syscall/arch/aarch64.rs`
- Result: rseq is now handled but issue persists

### 2. Debug Output Analysis
When debug println! was added to task.rs, the following was observed:
- Init process starts (Thread::run() called)
- Syscalls are made (rseq, mmap, mprotect, brk, etc.)
- Many page faults occur (DataAbortLowerEL, InstructionAbortLowerEL)
- Process crashes during glibc dynamic linking

### 3. Key Finding
The page faults happen during glibc library loading. The page fault handler
appears to not properly resolve some memory accesses, causing the process to crash.

## Files Changed
```
kernel/src/syscall/rseq.rs (NEW)
kernel/src/syscall/mod.rs (+ mod rseq)
kernel/src/syscall/arch/aarch64.rs (+ SYS_RSEQ = 293)
```

## Next Steps
1. Investigate why page faults occur on QEMU but not RPi3
2. Check memory layout differences between platforms
3. Review page fault handler for QEMU virt specific issues
4. Consider if this is a regression from a previous fix
