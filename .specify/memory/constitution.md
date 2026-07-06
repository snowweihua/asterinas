<!--
Sync Impact Report
==================
Version change: 1.2.0 → 1.3.0 (single-maintainer governance)
Modified principles: None
Added sections: None
Removed sections: None
Templates requiring updates: N/A
Follow-up TODOs: None
Changed sections:
  - Governance / Amendment Procedure: 2/3 maintainer approval → single-maintainer self-approval
  - Governance / Compliance Verification: updated to single-maintainer scope
-->

# Asterinas Constitution

## Core Principles

### I. Memory Safety Through Rust
The kernel MUST use Rust as its sole implementation language.
Unsafe code MUST be restricted to a clearly delineated and minimal Trusted Computing Base (TCB).
The framekernel architecture MUST isolate unsafe code from safe components.
Every unsafe block MUST be documented with rationale explaining why safe alternatives were rejected.

*Rationale*: Memory safety bugs are the leading cause of kernel vulnerabilities. By restricting unsafe Rust to a minimal TCB, Asterinas reduces the attack surface while retaining Rust's performance and expressiveness.*

### II. Linux ABI Compatibility
Asterinas MUST maintain a Linux-compatible Application Binary Interface (ABI).
System calls MUST follow Linux conventions for arguments, error codes, and data structures.
Any deviation from Linux ABI MUST be documented with justification and migration path.

*Rationale*: Linux compatibility enables Asterinas to run existing Linux binaries and serve as a seamless Linux replacement. This is the primary value proposition for users.*

### III. Small and Sound TCB
The Trusted Computing Base (TCB) MUST be kept small and formally verified where possible.
All unsafe code MUST be auditable and isolated in well-defined architectural boundaries.
New TCB additions MUST be reviewed by at least two maintainers and include a security justification.

*Rationale*: A smaller TCB means a smaller attack surface. Research (Asterinas ATC'25 paper) demonstrates that a small TCB enables stronger correctness guarantees.*

### IV. AArch64 Architecture Support
Asterinas on AArch64 MUST support QEMU virt machine (Cortex-A72, GICv3) for emulation and Raspberry Pi 3 Model B for real hardware.
Architecture-specific code MUST be isolated in arch-specific modules (ostd/src/arch/aarch64/).
AArch64-specific constraints:
- Use QEMU virt machine for emulation (Cortex-A72, GICv3)
- ARM Generic Timer: CNTP for QEMU, CNTV for RPi3 hardware
- BCM2835/BCM2836 IRQ router for RPi3 peripheral routing
- PL011 UART at 115200 baud for RPi3 serial console
- Use kernel linear map VA (e.g., 0xffff_8000_0900_0000) not raw PA after MMU enables

*Rationale*: AArch64 support is the current development focus. QEMU virt machine provides fast emulation iteration; RPi3 3B provides real hardware validation for bare-metal bring-up.*

### V. Rigorous Testing and CI
All pull requests MUST pass lint and compilation before merge.
AArch64 CI MUST include:
- **Lint**: ./tools/format_all.sh --check
- **Compilation**: cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64
- **Boot test**: QEMU virt (cortex-a72) with initramfs, verified via / # prompt
- **Real hardware**: RPi3 3B boot validation via serial console at 115200 baud

AArch64-specific boot command: qemu-system-aarch64 -machine virt -cpu cortex-a72 -smp 1 -m 512M -kernel <elf> -dtb <dtb> -append "console=ttyAMA0" -nographic -display none

*Rationale*: OS kernels require high reliability. AArch64 CI covers lint, compile, QEMU boot, and real RPi3 hardware validation. Other architectures (x86-64, RISC-V, LoongArch) are outside the scope of this task.*

## Security Requirements

### Unsafe Code Governance
- All unsafe code MUST reside in ostd/src/arch/, kernel/comps/virtio/, or other clearly marked TCB modules.
- Unsafe code MUST NOT leak across module boundaries without explicit, documented interfaces.
- The unsafe code budget (lines, modules) MUST be tracked and reviewed quarterly.

### Vulnerability Disclosure
- Security vulnerabilities MUST be reported via GitHub Security Advisories, not public issues.
- Critical vulnerabilities MUST be acknowledged within 48 hours and patched within 14 days.

## Development Workflow

### Code Review Requirements
- All commits MUST pass lint and CI before being considered complete.
- Architecture changes (ostd/src/arch/aarch64/) require self-review of the diff before commit.
- Breaking ABI changes MUST include a deprecation notice and 90-day migration window.

### Documentation Standards
- All public APIs MUST have rustdoc comments.
- Architecture decisions MUST be documented in The Asterinas Book (book/src/).
- Breaking changes MUST update the changelog under CHANGELOG.md.

## Governance

### Amendment Procedure
Constitution amendments require:
1. A commit proposing the change with rationale in the commit message
2. Version increment (MAJOR for breaking changes, MINOR for additions, PATCH for clarifications)

Note: As the sole maintainer, no external review is required. Rationale must still be documented for future maintainers.

### Compliance Verification
Self-review of all changes MUST verify:
- Unsafe code additions are justified and isolated
- Linux ABI compatibility is maintained
- Architecture-specific code within ostd/src/arch/aarch64/ does not break compilation on QEMU virt or RPi3 targets
- Tests pass on CI (lint, cargo build, QEMU boot)

**Version**: 1.3.0 | **Ratified**: TODO(RATIFICATION_DATE): Original adoption date unknown; project predates constitution | **Last Amended**: 2026-07-03
