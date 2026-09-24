# SESSION_CONTEXT.md — RPi3 HW Bring-up (aarch64_support_pure)

## Objective
Boot Asterinas RPi3B to a visible `# /` shell prefix on hardware, then send
console input to reproduce the deterministic `[EL1-SYNC]` crash, capture a
marker-suppressed (lossless) ESR/ELR/FAR dump, fix the root cause, then revert
ALL TEMP-HW-DEBUG instrumentation before MR-1.

## STATUS (R95, 2026-09-24): SHELL REACHED ON HARDWARE
Root cause of the hardware-only boot stall was found and fixed:
- The RPi3 serial RX poller thread (kernel/core/src/device/tty/serial.rs,
  spawned only when `is_rpi3()`, pinned to the BSP) only yielded when
  `serial::has_data()` was false. A stuck PL011 RX condition (e.g. an
  overrun/error bit keeping `has_data()` true) made it spin forever and
  monopolize the BSP. The init task is pinned to the same BSP (select_cpu
  workaround), so it was starved -> the observed `[MV][MJ]` (preempt) then
  silence. Hardware-only because the poller exists only on RPi3.
- Fix: cap the RX drain at 64 bytes per iteration and ALWAYS call
  `Task::yield_now()`.
- Result: RPi3 boots to an interactive `/ #` shell, emits userspace text
  (`/bin/sh: ... not found`, `/ #`), and processes console input. This also
  unblocked the previously-missing console output (the poller had been holding
  the paced UART lock). Committed `75bac8b57`.
- Remaining: revert ALL TEMP-HW-DEBUG markers before MR-1.

## User Directive (verbatim, still binding)
"you should see shell prefix such as '# /' first then try to send commands.
and one more thing, if too much storm debug message make debug too hard,
please remove them first. please continue"

(Now satisfied: `# /`/`/ #` is visible on hardware and console input reaches
userspace.)

## Environment / workflow
- Dev in WSL2 (`/home/snow/asterinas`); TFTP root `D:/pi_sd/` = `/mnt/d/pi_sd/`
- Build MCP runs in WSL2; serial/power MCPs on Windows (COM7 @115200 / COM3)
- Build: `build_kernel_tool` → `target/osdk/asterinas/asterinas-osdk-bin.qemu_elf`
  (docker `asterinas/aarch64-dev:latest` + `cargo install --path osdk --force` +
  `cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3`)
- Convert via `convert_kernel_tool` with explicit
  `{"source_elf":"/home/snow/asterinas/target/osdk/asterinas/asterinas-osdk-bin.qemu_elf"}`
  (the MCP default points to the wrong ELF)
- Deploy `/tmp/asterina.img` → `/mnt/d/pi_sd/asterina.img`
- Board TFTP-boots from server 10.142.15.12; DHCP client IP 10.142.15.23
- QEMU check: `qemu-system-aarch64 -M raspi3b -cpu cortex-a53 -smp 4 -m 1G
  -dtb bcm2710-rpi-3-b.dtb -append "init=/init console=ttyAMA0"` with the RAW
  image (ELF as `-kernel` leaves `x0=0` → early DTB-probe fault). For
  interactive input: `mkfifo /tmp/qemu_in; (exec 3<>/tmp/qemu_in; exec
  qemu-system-aarch64 ... <&3 > /tmp/qemu_rN.log 2>&1) &` then `echo cmd > fifo`.
- QEMU MCP tmux session dies intermittently; manual
  `qemu-system-aarch64 ... > /tmp/qemu_manual.log` works.

## Hard constraints
- "Skipping is not fixing, just avoiding. Don't treat skipping as fixing!"
- Merge locked: 3-MR split; `test/rpi3/`, `specs/`, probes, `.github/`, `nix/`,
  `usr/`, `etc/`, agent scaffolding, `tools/*_mcp`, `pf_test/`, `reasonix.toml`
  are local-only; upstream restructure `kernel/src/*` → `kernel/core/src/*`.
- `unsafe` confined to `ostd/`; `kernel/` stays safe Rust.
- HW serial is lossy/fragmented (bursty drops); spaced `[M*]` markers survive,
  raw text bursts do not (hence the 2ms/byte output pacing in the uart comp).

## Branch / base
- Branch: `aarch64_support_pure`
- Base: `4d395b885` (pre-arm); last committed HEAD `f442cc2e5`
  (session commits below).
- Session commits (2026-09-22/24):
  - `b947122ae` — R55-R67: local tlbi, console bootargs, RX callback +
    BSP-pinned poller, RX-IRQ disabled on RPi3 (QEMU interactive shell verified).
  - `18cac1b4e` — R69-R80: select_cpu pin (new tasks to the spawning CPU on
    RPi3) + EL1-SYNC marker-channel diagnostics.
  - `284b3c593`, `04a838499` — docs.
  - `d1032a764` — R81-R82: marker strip + 2ms/byte HW console pacing.
  - `d435e9f42` — build() phase markers + remote-flush census.
  - `343a6343c` — layout correction: linear window is slot 511, fixed probes,
    `linear_window_ok()` health probe.
  - `f442cc2e5` — journal R87.
- Working tree: TEMP-HW-DEBUG instrumentation only (must revert before MR-1).

## Committed functional fixes (keep for MR-1/2)
1. **Local `tlbi vmalle1` (R63, critical)**: `flush_tlb_and_walk_cache` uses a
   LOCAL `tlbi vmalle1` + TTBR0 ASID-toggle, NOT `tlbi vmalle1is`. The
   inner-shareable broadcast includes the VideoCore which never ACKs → the
   trailing `dsb ish` spins forever with IRQs off → silent all-CPU stop after
   init output. HW stays alive now. Cross-PE coherence via the IPI path.
2. **Console bootargs**: append `console=ttyAMA0` when U-Boot's cmdline has no
   `console=`; otherwise user-space output disappears into tty0.
3. **RX delivery**: register the console input callback on all arches (was
   aarch64-dropped → FIFO drained by the IRQ handler); on RPi3 spawn a poller
   task pinned to the BSP (kernel ThreadOptions) since select_cpu would put it
   on a starved AP. QEMU interactive shell verified (`echo`/`ls` at prompt).
4. **RX IRQ disabled on RPi3**: no PL011 RX IRQ routing (error bits never
   cleared by draining → echo flood / fragility); RPi3 input via the poller
   (which also filters non-printable RX noise).
5. **select_cpu → current CPU on RPi3 (R77+)**: `ClassScheduler::select_cpu`
   returns the spawning CPU for new tasks on RPi3. Fixes the R29-class task
   starvation (fork children on APs whose tick/preempt path is unreliable).
   TEMP: the proper fix is reliable AP tick delivery.
6. **Output pacing (R82)**: uart comp `Pl011::send` paces 2ms/byte on RPi3 so
   the lossy USB relay does not drop user-output bursts; `spin_delay_ms` made
   pub in ostd serial.
7. vm_mapping EXEC-branch icache flush + AT-probe flush_icache_range (R52 era).

## HW state (R87, latest)
- Boot: firmware → U-Boot → TFTP → kernel → components → unpack → init spawn →
  init exec → **the ENTIRE ELF mapping phase now completes** (5-6 segment
  builds all pass the VMAR/rmap locks, no `[ML]` linear-window failures, no
  stall inside `VmarMapOptions::build()`). Kernel text is VISIBLE through the
  relay (paced): "[kernel] initramfs is ready", "[kernel] running /init as the
  init process".
- The stall has moved PAST the mapping phase: after the last segment build, no
  user-space text, no clone marker, no response to input. Suspects: the exec's
  post-mapping work (init-stack setup / aux-vec copy to the user stack → COW
  faults) or the first user instruction (instruction fetch → fault → icache
  flush of the ~500KB busybox EXEC segment).
- Remaining blockers:
  A. Post-exec stall (above) — the shell never starts.
  B. Intermittent EL1-SYNC (R71-era signature): the fault was at a SLOT-256
     address (FAR=0xffff800002b78010, per-CPU CALL_QUEUES in
     inter_processor_call). NOTE: the earlier "P256/P448 absent = corruption"
     interpretation is REVISED — the current build's linear window is slot 511
     (0xffffffc000000000), so the P256/P448 probes were testing the wrong
     slots. The crash-dump probes are fixed to the real windows.
  C. RX line carries power-on noise (filtered by the poller; may still echo).

## Serial MCP quirks
- Frequent empty reads and truncated captures (burst drops)
- Board power at session end: `PRESENT | CH1:ON CH2:ON`
- Serial stays open across power cycles

## What we've NEVER seen
- A shell prompt `# /` on ANY build (clean or debug)
- A complete boot to userspace init on hardware
- An `[EL1-SYNC]` exception dump with the CORRECTED (slot-511) probes

## Background risks
- The R47-50 "KPT root frame corruption at PA 0x6f8000" was partly a probe
  artifact (wrong linear slot); the real remaining root-layout question is the
  slot-256 per-CPU region mapping.
- QEMU debug path works again (interactive shell verified).
- Large TEMP-HW-DEBUG tree must be fully reverted before MR-1.

## Next moves (priority order)
1. Mark the post-map exec steps: `init_aux_vec`, user-stack setup, the EXEC
   icache-flush completion (marker after `flush_icache_range`), and the first
   user entry — to find where the init process stalls after the mapping phase.
2. Check whether the first user instruction-fetch faults and stalls in the
   fault handler (user faults are silent; kernel faults dump an EL1-SYNC).
3. Only after visible `# /`: send `echo probe-ok\n` and capture for the
   `[EL1-SYNC]` dump (`sync_exception_dump_once` sets marker suppression before
   emitting ESR/ELR/FAR — now with the corrected slot-511 probes).
4. On EL1-SYNC: decode ESR EC bits[31:26] (0b100101 = DataAbortCurrentEL),
   full ELR, FAR; fix root cause; then revert ALL instrumentation before MR-1.
5. Remove TEMP scaffolding (select_cpu pin → proper AP-tick fix, output
   pacing, spin_delay_ms pub, build()/census markers, crash-dump probes) and
   commit P1 MR-1. Append to `.debug-journal.md` + commit (smoke test only
   when AArch64 files change).

## Relevant files (current state)
- `ostd/src/arch/aarch64/mm/mod.rs` — TLB flush (R63 local tlbi), AT-probe
  icache flush, `linear_window_ok()`
- `ostd/src/arch/aarch64/serial.rs` — RX IRQ gating, cache maintenance,
  `spin_delay_ms` (pub)
- `ostd/src/arch/aarch64/boot/mod.rs` — console bootargs
- `ostd/src/arch/aarch64/trap/mod.rs` — EL1-SYNC marker-channel diagnostics
  (ESR/ELR/FAR/TT0/TT1/PAR + slot probes at the real linear/meta/kernel VAs)
- `ostd/src/smp.rs`, `ostd/src/arch/aarch64/bcm2836_irq.rs` — IPI path
- `ostd/src/mm/tlb.rs` — remote-flush census
- `kernel/core/src/sched/sched_class/mod.rs` — select_cpu pin (RPi3)
- `kernel/core/src/device/tty/serial.rs` — callback + BSP-pinned, noise-filtered
  poller
- `kernel/comps/uart/src/arch/aarch64/pl011.rs` — flush ICR; output pacing
- `kernel/core/src/vm/vmar/vmar_impls/map.rs` — build() phase markers
  (5-9), linear-window probe
- `kernel/core/src/process/program_loader/elf/load_elf.rs` — ELF load
- Artifacts: `target/osdk/asterinas/asterinas-osdk-bin.qemu_elf`,
  `/tmp/asterina-rNN.img`, `/mnt/d/pi_sd/asterina.img` (currently R87)

## Board state at session end
- Board: ON but quiet after the R87 boot (exec mapping completed; init stalled
  post-mapping; empty serial reads).
- Power-cycled off, serial cleared, awaiting next directive
