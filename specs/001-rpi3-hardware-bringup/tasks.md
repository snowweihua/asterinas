# Tasks: RPi3 Hardware Bringup

**Input**: Design documents from `specs/001-rpi3-hardware-bringup/`

**Prerequisites**: plan.md, spec.md, research.md, quickstart.md

**Tests**: Not applicable — manual hardware testing only (no automated tests for bare-metal RPi3 bringup)

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)

---

## Phase 1: Assessment (Verify Current State)

**Purpose**: Confirm current RPi3 boot state and understand which issues are present

- [ ] T001 [P] Build kernel and verify QEMU virt boot still works after any changes
- [ ] T002 [P] Boot RPi3 hardware and verify which issues are reproducible: D-cache hang, stat SIGSEGV, reboot hang, SMP failure
- [ ] T003 Verify initramfs is being loaded correctly by checking for `[unpack]` and `[rootfs]` probes in serial log

---

## Phase 2: User Story 1 - Reliable First Boot (Priority: P1) 🎯 MVP

**Goal**: Kernel boots reliably on RPi3 3B hardware without D-cache coherency failures

**Independent Test**: Power cycle RPi3 10 times — all 10 boots reach `/ #` prompt

### Implementation

- [ ] T004 [P] [US1] Add D-cache clean to point of coherency in `ostd/src/arch/aarch64/boot/boot.S` — add `dc cvac` for initramfs memory region before MMU enable, per research.md §1
- [ ] T005 [P] [US1] Add `ic iallu` (invalidate I-cache all) after MMU enable in `ostd/src/arch/aarch64/boot/boot.S`, per research.md §1
- [ ] T006 [US1] Rebuild kernel and deploy to RPi3 via TFTP
- [ ] T007 [US1] Run 10 consecutive power-on tests — verify 10/10 reach `/ #` prompt (SC-001)

**Checkpoint**: User Story 1 complete — first-boot reliability achieved

---

## Phase 3: User Story 2 - Stable Shell Interaction (Priority: P1)

**Goal**: `ls /bin` and `echo` work on RPi3 serial console without SIGSEGV

**Independent Test**: Run `ls /bin`, `echo hello`, `cat /proc/interrupts` — all succeed without crash (SC-003)

### Implementation

- [ ] T008 [P] [US2] Fix AArch64 `struct Stat` in `kernel/src/syscall/stat.rs` — move `__pad0` before `st_rdev`, ensure `st_size` at offset 44, remove or correctly place `__pad1`, per research.md §3
- [ ] T009 [P] [US2] Add `#![forbid(unsafe_code)]` lint suppress or document existing unsafe blocks if any are introduced by stat changes
- [ ] T010 [US2] Rebuild kernel and deploy to RPi3
- [ ] T011 [US2] Verify `ls /bin` succeeds without SIGSEGV (SC-003)
- [ ] T012 [US2] Verify `echo hello` echoes correctly
- [ ] T013 [US2] Verify `cat /proc/interrupts` shows serial and timer IRQs

**Checkpoint**: User Story 2 complete — shell interaction stable

---

## Phase 4: User Story 3 - SMP Secondary Core Bringup (Priority: P2)

**Goal**: At least 2 CPU cores detected online on RPi3 after boot

**Independent Test**: Boot kernel and verify `/proc/cpuinfo` or equivalent shows multiple online CPUs (SC-005)

### Implementation

- [ ] T014 [P] [US3] Add debug putchars in `ostd/src/arch/aarch64/boot/ap_boot.S` before and after MMU enable to trace AP execution
- [ ] T015 [P] [US3] Add debug putchars in `ostd/src/arch/aarch64/boot/smp_rpi3.rs` to verify spin-table writes and mailbox IRQ trigger
- [ ] T016 [US3] Analyze BCM2836 mailbox IRQ behavior — determine if `sev` alone wakes AP or if specific interrupt required
- [ ] T017 [US3] Fix `ostd/src/arch/aarch64/boot/smp_rpi3.rs` based on T015 analysis — verify AP info region reads correctly by AP after MMU enable
- [ ] T018 [US3] Rebuild and deploy to RPi3
- [ ] T019 [US3] Verify at least 2 CPU cores online via `/proc/cpuinfo` or equivalent (SC-005)

**Checkpoint**: User Story 3 complete — SMP bringup working

---

## Phase 5: User Story 4 - Clean Reboot (Priority: P3)

**Goal**: `reboot` syscall resets the system cleanly within 30 seconds

**Independent Test**: Invoke `reboot` and verify hardware resets within 30 seconds (SC-004)

### Implementation

- [ ] T020 [P] [US4] Create `kernel/src/syscall/reboot.rs` implementing `sys_reboot(cmd, arg)` with PSCI SYSTEM_RESET (0x84000009) for RB_RESTART/RB_AUTOBOOT, per research.md §4
- [ ] T021 [P] [US4] Register `mod reboot` in `kernel/src/syscall/mod.rs`
- [ ] T022 [P] [US4] Add `sys_reboot` to `kernel/src/syscall/arch/aarch64.rs` with syscall number 142
- [ ] T023 [US4] Rebuild and deploy to RPi3
- [ ] T024 [US4] Verify `reboot` syscall resets within 30 seconds (SC-004)

**Checkpoint**: User Story 4 complete — reboot works

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Cleanup and verification after all user stories

- [ ] T025 [P] Remove all debug `[unpack]`, `[rootfs]`, `[task-loop]`, `[drv]`, `[kt1]` probe print statements added during bring-up
- [ ] T026 [P] Remove any temporary debug putchars added in `ap_boot.S` and `smp_rpi3.rs` (T014, T015)
- [ ] T027 Run QEMU virt boot test to verify no regression: `qemu-system-aarch64 -machine virt -cpu cortex-a72 -smp 1 -m 512M -kernel target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf -dtb test/nix/aarch64-virt.dtb -device loader,file=test/build/virt-init.dtb,addr=0x47000000,force-raw=on -device loader,file=test/build/init.cpio.gz,addr=0x48000000,force-raw=on -append "console=ttyAMA0" -nographic -display none`
- [ ] T028 Run `./tools/format_all.sh --check` and fix any formatting issues
- [ ] T029 Document final RPi3 boot procedure in The Asterinas Book (`book/src/`) or as a README in `test/rpi3/`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Assessment)**: No dependencies — starts immediately
- **Phase 2 (US1)**: Can start immediately (independent of Phase 1 findings if D-cache fix is known)
- **Phase 3 (US2)**: Can start in parallel with Phase 2 — no dependencies between US1 and US2
- **Phase 4 (US3)**: Can start in parallel with Phase 2 and 3 — SMP fix is independent
- **Phase 5 (US4)**: Can start in parallel with Phase 2, 3, 4 — reboot syscall is independent
- **Phase 6 (Polish)**: Depends on all user stories complete

### User Story Dependencies

- **US1 (P1)**: Independent — D-cache fix only affects boot reliability
- **US2 (P1)**: Independent — stat struct fix does not affect boot
- **US3 (P2)**: Independent — SMP fix does not affect boot or shell
- **US4 (P3)**: Independent — reboot syscall does not affect boot, shell, or SMP

### Within Each User Story

- The two cache maintenance tasks (T004, T005) can run in parallel (different lines in boot.S)
- The two registration tasks for reboot (T021, T022) can run in parallel (different files)
- US2 stat struct task (T008) — no same-file conflicts

### Parallel Opportunities

All user stories can be implemented in parallel since they modify different files:
- **US1**: `ostd/src/arch/aarch64/boot/boot.S`
- **US2**: `kernel/src/syscall/stat.rs`
- **US3**: `ostd/src/arch/aarch64/boot/smp_rpi3.rs`, `ostd/src/arch/aarch64/boot/ap_boot.S`
- **US4**: `kernel/src/syscall/reboot.rs` + `kernel/src/syscall/mod.rs` + `kernel/src/syscall/arch/aarch64.rs`

---

## Implementation Strategy

### MVP First (US1 + US2)

1. Complete Phase 1: Assessment
2. Complete Phase 2: US1 (D-cache coherency fix)
3. Complete Phase 3: US2 (stat struct fix)
4. **STOP and VALIDATE**: Test on RPi3 hardware — shell should work reliably
5. US1 + US2 is the MVP — these unblock all further development

### Incremental Delivery

1. US1 + US2 → Test on RPi3 → MVP achieved
2. Add US3 (SMP) → Test on RPi3 → Multi-core working
3. Add US4 (reboot) → Test on RPi3 → Full bringup complete
4. Phase 6 (Polish) → All probe strings removed, docs updated

### Parallel Execution

With single developer working sequentially:

1. Complete Phase 1 (Assessment)
2. Implement US1 and US2 in sequence (different files, could be parallel)
3. Deploy and test on RPi3
4. Implement US3 (SMP) — most complex, requires careful debugging
5. Deploy and test
6. Implement US4 (reboot) — simple syscall addition
7. Deploy and test
8. Polish

---

## Notes

- All code changes must pass `./tools/format_all.sh --check` before commit
- QEMU virt regression test (T027) MUST pass after every phase before proceeding
- No automated unit tests for bare-metal bringup — all testing is manual on RPi3 hardware
- Probe removal (T025, T026) is critical before considering the feature complete
- After Phase 6, all four user stories should be independently testable on RPi3 hardware
