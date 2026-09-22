# SESSON_CONTEXT.md — aarch64_support_pure (RPi3 bring-up)

## Objective
Port AArch64 RPi3B support onto clean pre-arm base for upstream MR-1,
with only arch-scoped changes; keep QEMU `raspi3b` and HW RPi3B parity (SMP/shell).

## Branch / base
- Branch: `aarch64_support_pure`
- Base: `4d395b885` (pre-arm); last committed HEAD `6e85bf329`
  "aarch64/mm: probe AT before icache flush to skip unmapped pages (R52)".
- **Uncommitted delta (R55-R67) = "HW interactive-shell push"** — see the
  diff; the functional fixes are: local `tlbi vmalle1` (R63), console bootargs
  append, RX callback + BSP-pinned poller, RX-IRQ disabled on RPi3.

## Environment / workflow
- Dev in WSL2 (`/home/snow/asterinas`); TFTP root `D:/pi_sd/` = `/mnt/d/pi_sd/`;
  `/srv/tftp` is NOT the TFTP root. SD updates need manual user copy.
- Build MCP runs in WSL2; serial/power MCPs on Windows (COM7 @115200 / COM3).
- Build: docker `asterinas/aarch64-dev:latest` +
  `cargo install --path osdk --force` (image ships stale OSDK) +
  `cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3`
- Convert via build MCP `convert_kernel_tool` (objcopy ELF → raw binary),
  deploy to `/mnt/d/pi_sd/asterina.img`, then power off → serial clear → power on,
  wait ~90-150s, `serial_read` / `serial_wait`.
- QEMU check: `raspi3b -cpu cortex-a53 -smp 4 -m 1G` with raw image +
  `-dtb bcm2710-rpi-3-b.dtb -append "init=/init console=ttyAMA0"`.
  For interactive input: `mkfifo /tmp/qemu_in; (exec 3<>/tmp/qemu_in; exec
  qemu-system-aarch64 ... <&3 > /tmp/qemu_rN.log 2>&1) &` then
  `echo cmd > /tmp/qemu_in`.

## Hard constraints (do not violate)
- "Skipping is not fixing, just avoiding. Don't treat skipping as fixing!"
- Merge locked: 3-MR split; `test/rpi3/`, `specs/`, probes, `.github/`, `nix/`,
  `usr/`, `etc/`, agent scaffolding, `tools/*_mcp`, `pf_test/`, `reasonix.toml`
  are local-only; upstream restructure `kernel/src/*` → `kernel/core/src/*`.
- AGENTS.md: `unsafe` confined to `ostd/`; `kernel/` stays safe Rust.
- HW serial channel is lossy/fragmented (bursty drops). Long INFO lines truncate;
  short `[M*]` markers + compact panic text used for HW debug.

## Committed state (up to 6e85bf329, R52)
- `ostd/src/arch/aarch64/` + arch dispatch, unified KERNEL_CODE_BASE_VADDR,
  O(n) boot tables, aarch64.ld/aarch64-rpi3.ld, PL011 uart, bcm2836 IRQ.
- R51 `1d57a794d`: re-applied local-IC cache maintenance (`dc ivac/cvac` in
  bcm2836_irq mmio_read/write) — fixed the intermittent initramfs-unpacking hang.
- R52 `6e85bf329`: `flush_icache_range` AT-probe (`at s1e0r` + `par_el1`, skip
  unmapped lines) — fixed the "Cannot handle user page fault" panic.
- QEMU: full boot to `/ #` shell (AUTO-TEST runs, `echo`/`ls` work) — verified
  interactively in this session (R55+).

## Uncommitted delta R55-R67 — functional fixes (keep for MR-1/2)
1. **`ostd/src/arch/aarch64/mm/mod.rs` — LOCAL `tlbi vmalle1` (R63, critical)**:
   `flush_tlb_and_walk_cache` dropped the inner-shareable broadcast
   (`tlbi vmalle1is`). On BCM2836 the broadcast includes the VideoCore which
   never ACKs → the trailing `dsb ish` spins forever → BSP wedged with IRQs off
   → silent whole-system stop after init output. Local `tlbi vmalle1` + TTBR0
   ASID-toggle + the IPI flush path is the working combo. **HW stays alive now.**
2. **`ostd/src/arch/aarch64/boot/mod.rs` — console bootargs**: append
   `console=ttyAMA0` when the bootloader cmdline has no `console=` (U-Boot sets
   `init=/init` only) — otherwise SystemConsole falls back to tty0 and all
   user-space output is invisible.
3. **`kernel/core/src/device/tty/serial.rs` — RX delivery**:
   - register the console input callback on ALL arches (the aarch64-only cfg had
     dropped it; the PL011 RX IRQ handler drained the FIFO → input was lost).
   - on RPi3 (no RX IRQ — see #4) spawn a poller task PINNED TO THE BSP via
     kernel `ThreadOptions::cpu_affinity(CpuId::bsp())`. Pin is required:
     `ClassScheduler::select_cpu` puts new tasks on the least-loaded CPU (an AP)
     and RPi3 AP tick delivery is unreliable → spawned tasks starve (the R29
     devtmpfsd stall class). QEMU input verified with this poller.
4. **`ostd/src/arch/aarch64/serial.rs` + `kernel/comps/uart/.../pl011.rs` —
   RX IRQ disabled on RPi3**: `init_rx_irq`/`reenable_rx_irq`/pl011 `flush()`
   no longer enable the PL011 RX interrupt or the BCM2836 GPU IRQ routing on
   RPi3 (QEMU keeps it). Rationale: the RX line carries noise at power-on and
   error bits are never cleared by draining → echo flood + IRQ-path fragility.
   RPi3 input uses the poller instead.
5. vm_mapping.rs EXEC-branch `flush_icache_range` + TlbFlushOp additions
   (opencode R52-companion; keep).

## HW state (R67, after the above fixes)
- Boot: firmware → U-Boot → TFTP → kernel → component init → unpack → init
  spawn → console=serial → first user writes → **system STAYS ALIVE** (T
  heartbeat + idle; no silent stop; no unpack hang; no user-fault panic).
- The BSP-pinned poller RUNS on HW (PL1) and reads the PL011 RX FIFO.
- **OPEN BLOCKER A (crash)**: init's first fork+exec (`ls`/`sh` → ELF load)
  faults in `map_segment_vmos` (ELR 0xffffffff001c) on a LINEAR-map address
  (FAR 0xffff80000600/0x80000 → PA 0x60000/0x80000 = early RAM/kernel-image
  region) → `[EL1-SYNC]` halt. This is the R47-50 KPT root-frame corruption
  (linear window PGD[256] cleared at runtime) surfacing during ELF loading.
  Intermittent but frequent now. QEMU completes the same path.
- **OPEN BLOCKER B (garbage)**: HW RX line carries noise at boot; the poller
  reads 0x00-ish bytes; with push_input enabled → TTY echo flood (`@@@@`).
  Drain-at-start + discard currently in the poller (debug); the crash (A)
  happens regardless of echo.
- **OPEN BLOCKER C (init output)**: even without a crash, init produces no
  visible AUTO-TEST output on HW (only early WARN fragments) — the ELF exec
  blocks or faults before the script output.

## Round log (this session, R55-R67)
- R55: QEMU interactive shell VERIFIED (first time) — input callback fix.
- R56-R59: HW silent all-CPU stop after init output; census markers; not RX.
- R60-R62: RX-IRQ disable test; poller-starves discovery (select_cpu → AP).
- R63: **vmalle1is broadcast → local vmalle1 — silent stop FIXED**.
- R65: poller pinned to BSP; QEMU input verified again.
- R65-R67: HW ELF-load linear-map fault (blocker A) + RX garbage (blocker B).
- Full details in `.debug-journal.md` R55-R67.

## Next moves
1. Blocker A: hunt the KPT root-frame corruption (R47-50, now reproducible via
   the ELF load). Candidates: KPT root frame refcount/lifetime (root freed and
   reused as heap → PGD[256]/[448] zeroed), frame allocator double-issue,
   or cursor operating on the wrong root (COW `cursor.unmap`). Instrument:
   snapshot the active root's PGD[256]/[448] around ELF load; log the KPT root
   frame's allocator state; check `activate_kernel_page_table` root refholding.
2. Blocker B: identify the RX garbage source (PL011 FIFO vs stale FR read vs
   line noise); consider clearing ICR + FIFO at poller start and gating echo.
3. Blocker C: with A+B fixed, confirm AUTO-TEST → `/ #` on HW, then `echo`/`ls`.
4. Remove TEMP-HW-DEBUG scaffolding (markers, log level, suppression, probes);
   re-enable virtio correctly (IPI/TLB ordering); commit P1 MR-1.

## Relevant files
- `ostd/src/arch/aarch64/mm/mod.rs` (TLB flush — R63 fix, AT-probe icache)
- `ostd/src/arch/aarch64/serial.rs` (RX IRQ gating, cache maintenance)
- `ostd/src/arch/aarch64/boot/mod.rs` (console bootargs)
- `ostd/src/arch/aarch64/timer/mod.rs`, `trap/mod.rs`, `bcm2836_irq.rs`
- `kernel/core/src/device/tty/serial.rs` (callback + BSP-pinned poller)
- `kernel/comps/uart/src/arch/aarch64/pl011.rs` (flush ICR; RX IRQ gating)
- `kernel/core/src/process/program_loader/elf/load_elf.rs` (ELF-load fault site)
- `kernel/core/src/vm/vmar/vm_mapping.rs` (fault handler; EXEC icache flush)
- Artifacts: `target/osdk/asterinas/asterinas-osdk-bin.qemu_elf`,
  `/tmp/asterina-rNN.img`, `/mnt/d/pi_sd/asterina.img` (currently R67)
