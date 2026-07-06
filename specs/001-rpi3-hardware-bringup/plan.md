# Implementation Plan: RPi3 Hardware Bringup

**Branch**: `001-rpi3-hardware-bringup` | **Date**: 2026-07-03 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-rpi3-hardware-bringup/spec.md`

## Summary

Asterinas on RPi3 3B hardware boots to a shell prompt but suffers from first-boot reliability failures (D-cache coherency gap during CPIO extraction), `ls /bin` SIGSEGV (stat/lstat struct layout mismatch), broken SMP secondary core bringup (BCM2836 spin-table not working), and missing reboot syscall (169). This plan addresses all four issues plus documents remaining cleanup.

## Technical Context

**Language/Version**: Rust (nightly-2025-02-01 toolchain, ostd framework)

**Primary Dependencies**: Asterinas OSTD (ostd crate), fdt crate for device tree parsing, BCM2836 peripheral interface

**Storage**: N/A (bare-metal, no persistent storage in this feature)

**Testing**: Manual boot test on RPi3 hardware via serial console at 115200 baud; QEMU virt for regression testing

**Target Platform**: AArch64 bare-metal on Raspberry Pi 3 Model B (BCM2837 SoC, 4x Cortex-A53)

**Project Type**: OS kernel feature (bare-metal bringup, not application)

**Performance Goals**: Boot to shell in under 30 seconds; reboot in under 30 seconds; 100% boot success rate over 10 attempts

**Constraints**:
- Kernel must use only Rust-safe code outside arch-specific TCB modules
- All unsafe blocks must be documented with rationale
- Architecture-specific code must stay in ostd/src/arch/aarch64/
- Two-stage boot: VideoCore → U-Boot (kernel8.img) → Asterinas (asterina.img) via booti

**Scale/Scope**: Single-node 4-core ARM64, no network, serial console only

## Constitution Check

*GATE: Verified post-research. All issues resolved or with clear fix path.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Memory Safety Through Rust | ✅ PASS | All new code uses safe Rust; unsafe only in arch modules; cache maintenance via inline asm |
| II. Linux ABI Compatibility | ⚠️ Needs resolution | stat struct `__pad0` placement wrong for AArch64; initial fix reverted due to userspace ABI regression — busybox binary expects wrong layout; see quickstart.md troubleshooting |
| III. Small and Sound TCB | ✅ PASS | No new TCB additions; existing arch modules only |
| IV. AArch64 Architecture Support | ✅ PASS | Code lives in ostd/src/arch/aarch64/; CNTV timer, BCM2836 IRQ, PL011 UART already implemented |
| V. Rigorous Testing and CI | ✅ PASS | Manual RPi3 hardware testing in quickstart.md; QEMU regression documented |

**Violations requiring justification**:
- None

## Project Structure

### Documentation (this feature)

```text
specs/001-rpi3-hardware-bringup/
├── plan.md              # This file
├── research.md          # Phase 0: D-cache coherency, SMP bringup analysis
├── data-model.md        # Phase 1: Not applicable (no data model for kernel bringup)
├── quickstart.md        # Phase 1: RPi3 boot walkthrough
└── tasks.md             # Phase 2: Task list (via /speckit.tasks)
```

### Source Code (repository root)

```text
ostd/src/arch/aarch64/
├── boot/
│   ├── mod.rs           # Entry point, DTB parsing, initramfs parsing
│   ├── boot.S           # Primary boot assembly
│   ├── ap_boot.S        # AP boot stub (for SMP)
│   ├── smp.rs           # Generic SMP bringup (QEMU path)
│   └── smp_rpi3.rs      # RPi3 BCM2836 spin-table SMP bringup
├── timer/mod.rs         # CNTV timer for RPi3
├── serial.rs            # PL011 UART (TX + RX interrupt)
├── irq.rs               # IRQ handling framework
├── gic.rs               # GICv3 interrupt controller (QEMU virt)
├── bcm2836_irq.rs       # BCM2835/2836 IRQ router (RPi3)
├── mm/                  # Page table, memory management
└── trap/                # Exception handling, ex_table

kernel/src/syscall/
├── reboot.rs            # MISSING — syscall 169 to implement

kernel/src/device/       # Device drivers
kernel/src/driver/mod.rs # Driver framework
```

**Structure Decision**: This feature modifies existing AArch64 arch code and adds one new syscall. No new directories or crates required.

## Phase 0: Research

**Status**: ✅ Complete — see [research.md](./research.md) for all findings.

### Findings Summary

1. **D-cache coherency**: U-Boot uses cached DRAM writes; `dc cvac` + `ic iallu` needed before kernel parses initramfs (research.md §1)
2. **BCM2836 SMP**: Two-phase spin-table protocol correct in code; AP never wakes — likely BCM2836 mailbox IRQ not reaching AP (research.md §2)
3. **stat struct**: `__pad0` placed AFTER `st_rdev` in Asterinas vs BEFORE in Linux — shifts all fields by 12 bytes. Initial fix caused userspace ABI regression (stack smashing); fix reverted. The stat struct issue remains open and needs resolution. See quickstart.md troubleshooting.
4. **reboot syscall**: Syscall 142 not mapped; needs new `reboot.rs` using PSCI `SYSTEM_RESET` (research.md §4)

---

## Phase 1: Design

### Output: research.md ✅
All 4 research questions resolved.

### Output: quickstart.md ✅
RPi3 boot walkthrough with build commands, boot test procedure, and troubleshooting table.

### Output: data-model.md
Not applicable — this is a kernel bringup feature with no persistent data model.

---

## Complexity Tracking

No constitution violations requiring justification.

| Area | Why Needed | Simpler Alternative Rejected Because |
|------|------------|--------------------------------------|
| BCM2836 spin-table protocol | RPi3 doesn't support PSCI for SMP; hardware-level spin-table is the only way | No alternative — PSCI not available on RPi3 |
| D-cache maintenance before initramfs parse | U-Boot uses cached DRAM writes; without explicit `dc cvac`, initramfs data may be stale when kernel reads it | Can't disable D-cache on U-Boot; would destroy performance |
| AArch64 stat struct fix | Must match Linux kernel struct stat layout exactly, or musl busybox will read/write wrong offsets | No alternative — Linux ABI compatibility requires correct struct layout. NOTE: initial fix caused stack smashing regression and was reverted. The stat struct issue remains unresolved. |
| PSCI reboot | RPi3 has no PMIC; only way to reset is via ARM Trusted Firmware PSCI call | No alternative — hardware reset requires PSCI |
