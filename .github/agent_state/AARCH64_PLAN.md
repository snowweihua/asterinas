# AArch64 Development Plan

## Current Status (2026-04-17)

### Completed ✅
- P0.1 Reproducible check
- P0.2 Trap path coherence  
- P1.1 Boot banner appears
- P1.2 Timer IRQ liveness
- P1.3 Panic handler (basic - stack trace disabled)
- P2.1 User transition round-trip
- P2.2 Syscall smoke
- P2.3 Page fault handoff
- P3.1 Kernel arch wiring
- P4.1 OSDK build path
- P4.2 OSDK run path
- Build fixes (edition, arm-gic)
- .gitignore updates
- Documentation

---

## TBD Tasks (Prioritized)

### P1 - Critical (Blocking Development)

#### P1.1 Restore Debug Output
- **Priority:** HIGH
- **Status:** Commit 05ea31fd removed all debug UART probes
- **Impact:** Hard to debug issues without output
- **Task:** Add configurable debug flags/markers
- **Files:** `ostd/src/arch/aarch64/boot/*.rs`

#### P1.2 Docker Build Determinism
- **Priority:** HIGH  
- **Status:** Docker build sometimes produces different binary
- **Impact:** Inconsistent test results
- **Task:** Investigate root cause, ensure reproducible builds
- **Related:** Cargo.lock, build cache issues

---

### P2 - Core Features (High Priority)

#### P2.1 SMP/Multicore Support
- **Priority:** HIGH
- **Status:** PSCI CPU_ON implemented but not tested
- **Task:** 
  - Verify secondary CPU boot
  - Test CPU hotplug
  - Test multicore scheduling
- **Files:** `ostd/src/arch/aarch64/boot/smp.rs`

#### P2.2 Syscall Subset Testing (P3.2)
- **Priority:** HIGH
- **Status:** Only basic syscalls tested
- **Task:** Test complete syscall interface:
  - File operations (read, write, open, close)
  - Process management (fork, exec, exit)
  - Memory (mmap, munmap)
  - Networking (if available)
- **Files:** `kernel/src/syscall/*`

#### P2.3 Poweroff/Restart
- **Priority:** MEDIUM
- **Status:** Not verified
- **Task:** Test clean shutdown
- **Files:** `ostd/src/arch/aarch64/shutdown.rs`

---

### P3 - Quality of Life

#### P3.1 Interrupt Handling Improvements
- **Priority:** MEDIUM
- **Status:** Basic IRQ handling works
- **Task:**
  - Test device interrupts (virtio, timer)
  - IRQ affinity/spread
- **Files:** `ostd/src/arch/aarch64/irq.rs`

#### P3.2 Memory Management
- **Priority:** MEDIUM
- **Status:** Basic working
- **Task:**
  - Test more page fault scenarios
  - Memory allocation stress test
  - Swap (if implemented)
- **Files:** `ostd/src/mm/*`

---

### P4 - Testing & Debugging

#### P4.1 Automated Tests
- **Priority:** MEDIUM
- **Status:** Manual testing only
- **Task:** Create CI tests for:
  - Boot test
  - Syscall test
  - Benchmark baseline

#### P4.2 GDB Debugging
- **Priority:** LOW
- **Status:** Not set up
- **Task:** Enable GDB stub for kernel debugging

---

### P5 - Long Term

#### P5.1 Hardware Support
- **Priority:** LOW
- **Status:** QEMU only
- **Tasks:**
  - Raspberry Pi 3B/4 support
  - Real hardware boot

#### P5.2 VirtIO Drivers
- **Priority:** LOW
- **Status:** Basic
- **Tasks:**
  - VirtIO block
  - VirtIO network
  - VirtIO console

---

## Quick Wins (Can Do Now)

1. **Add back minimal debug markers** - Helps with further debugging
2. **Document syscall interface** - What syscalls work?
3. **Run sysbench** - Get performance baseline
4. **Test with larger initramfs** - Verify memory handling

---

## Next Immediate Actions

### Step 1: Add Debug Output (P1.1)
```
HIGH PRIORITY - Blocks other debugging
```

### Step 2: Verify SMP (P2.1)  
```
HIGH PRIORITY - Core feature
```

### Step 3: Test Syscall Subset (P2.2)
```
HIGH PRIORITY - Quality gate
```

---

## File Locations

### AArch64 Code
- Boot: `ostd/src/arch/aarch64/boot/`
- CPU: `ostd/src/arch/aarch64/cpu/`
- MM: `ostd/src/arch/aarch64/mm/`
- Serial: `ostd/src/arch/aarch64/serial.rs`
- Timer: `ostd/src/arch/aarch64/timer/`
- Task: `ostd/src/arch/aarch64/task/`

### Kernel Code
- Syscall: `kernel/src/syscall/`
- Process: `kernel/src/process/`
- FS: `kernel/src/fs/`
- Net: `kernel/src/net/`

---

*Updated: 2026-04-17*