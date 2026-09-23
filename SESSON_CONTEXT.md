# SESSON_CONTEXT.md — aarch64_support_pure (RPi3 bring-up)

## Objective
Port AArch64 RPi3B support onto clean pre-arm base for upstream MR-1,
with only arch-scoped changes; keep QEMU `raspi3b` and HW RPi3B parity (SMP/shell).

## Branch / base
- Branch: `aarch64_support_pure`
- Base: `4d395b885` (pre-arm); last committed HEAD `d1032a764`
  "aarch64: strip debug markers, pace HW console output — clean text path".
- Session commits: `b947122ae` (R55-R67: local tlbi/console/RX fixes),
  `18cac1b4e` (R69-R80: select_cpu pin + EL1-SYNC diagnostics),
  `284b3c593` (docs), `d1032a764` (R81-R82: marker strip + output pacing).

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
  Interactive input: `mkfifo /tmp/qemu_in; (exec 3<>/tmp/qemu_in; exec
  qemu-system-aarch64 ... <&3 > /tmp/qemu_rN.log 2>&1) &` then `echo cmd > fifo`.

## Hard constraints (do not violate)
- "Skipping is not fixing, just avoiding. Don't treat skipping as fixing!"
- Merge locked: 3-MR split; `test/rpi3/`, `specs/`, probes, `.github/`, `nix/`,
  `usr/`, `etc/`, agent scaffolding, `tools/*_mcp`, `pf_test/`, `reasonix.toml`
  are local-only; upstream restructure `kernel/src/*` → `kernel/core/src/*`.
- AGENTS.md: `unsafe` confined to `ostd/`; `kernel/` stays safe Rust.
- HW serial channel is lossy/fragmented (bursty drops); spaced markers survive,
  raw text bursts do not (hence the 2ms/byte output pacing in the uart comp).

## Committed functional fixes (keep for MR-1/2)
1. **Local `tlbi vmalle1` (R63, critical)**: `flush_tlb_and_walk_cache` uses a
   LOCAL `tlbi vmalle1` + TTBR0 ASID-toggle, not `tlbi vmalle1is`. The
   inner-shareable broadcast includes the VideoCore (never ACKs) → the trailing
   `dsb ish` spun forever with IRQs off → silent all-CPU stop after init output.
   HW stays alive now. Cross-PE coherence via the IPI path.
2. **Console bootargs**: append `console=ttyAMA0` when U-Boot's cmdline has no
   `console=`; otherwise user-space output disappears into tty0.
3. **RX delivery**: register the console input callback on all arches (was
   aarch64-dropped → FIFO drained by the IRQ handler); on RPi3 spawn a poller
   task pinned to the BSP (kernel ThreadOptions) since select_cpu would put it
   on a starved AP. QEMU interactive shell verified.
4. **RX IRQ disabled on RPi3**: no PL011 RX IRQ routing (error bits never
   cleared by draining → echo flood / fragility); RPi3 input via the poller.
5. **select_cpu → current CPU on RPi3 (R77+)**: `ClassScheduler::select_cpu`
   returns the spawning CPU for new tasks on RPi3. Fixes the R29-class task
   starvation (fork children on APs whose tick/preempt path is unreliable).
   AUTO-TEST now progresses (init shebang + ls / + ls /bin + /bin/sh execs
   complete; interactive input reaches the shell). TEMP: the proper fix is
   reliable AP tick delivery.
6. **Output pacing (R82)**: uart comp `Pl011::send` paces 2ms/byte on RPi3 so
   the lossy USB relay does not drop user-output bursts; `spin_delay_ms` made
   pub in ostd serial.
7. vm_mapping EXEC-branch icache flush + AT-probe flush_icache_range (R52 era).

## HW state (R82)
- Boot: firmware → U-Boot → TFTP → kernel → components → unpack → init spawn →
  exec (map steps complete SOMETIMES; intermittently stalls inside
  `vm_map_options.build()` → the exec never finishes → no script output).
- R80 (lucky boot): 4 exec runs completed (AUTO-TEST progressed), poller ran,
  interactive "ls" input triggered an exec (RX works end-to-end).
- Remaining blockers:
  A. **Intermittent exec-map stall** inside build() (between the VMAR write
     lock/region alloc/VMO rmap/page-cache paths). Not the IPI path (with the
     select_cpu pin the exec flush is local-only). Same intermittent class as
     the pre-R51 unpack hang.
  B. **Intermittent EL1-SYNC** (R71-style): level-0 translation fault on the
     linear+meta windows of the active TTBR1 root (R47-50 "impossible
     pattern"), hit inside `IpiSender::inter_processor_call` (per-CPU
     CALL_QUEUES at PA ~43MB). Diagnostics: ESR/ELR/FAR/TT0/TT1/PAR/P256/P448/
     P511/KPT via the marker channel.
  C. RX line carries power-on noise (poller drains at start; may echo garbage).
- Crash-dump instrumentation kept (fires only on EL1-SYNC). All other debug
  markers removed (clean tree).

## Round log
- R51-52: unpack hang fix (local-IC cache maintenance), user-fault fix
  (AT-probe icache) — committed upstream of this session.
- R55-R67: QEMU shell verified; HW silent stop root-caused (vmalle1is) & fixed.
- R69-R73: EL1-SYNC diagnostics; linear-window L0 fault identified.
- R77-R80: select_cpu pin; init script progresses; RX reaches shell.
- R81-R82: marker strip + output pacing; exec-map stall characterized.
- Full details in `.debug-journal.md`.

## Next moves
1. Blocker A (exec-map stall): instrument `VmarMapOptions::build` phases (VMAR
   write lock, region alloc, VMO rmap lock, insert_try_merge, page-cache
   commit_on) + the frame allocator GLOBAL_POOL; compare against the pre-R51
   collect_pages hang. Consider disabling the poller to test interference.
2. Blocker B: with A fixed, re-examine the linear-window root corruption.
3. Verify the HW shell interactively once the exec completes reliably.
4. Remove TEMP scaffolding (select_cpu pin → proper AP-tick fix, output pacing,
   spin_delay_ms pub, crash-dump markers) + commit P1 MR-1.

## Relevant files
- `ostd/src/arch/aarch64/mm/mod.rs` (TLB flush R63, AT-probe icache)
- `ostd/src/arch/aarch64/serial.rs` (RX IRQ gating, cache maintenance,
  spin_delay_ms pub)
- `ostd/src/arch/aarch64/boot/mod.rs` (console bootargs)
- `ostd/src/arch/aarch64/trap/mod.rs` (EL1-SYNC marker-channel diagnostics)
- `ostd/src/smp.rs`, `ostd/src/arch/aarch64/bcm2836_irq.rs` (IPI path)
- `kernel/core/src/sched/sched_class/mod.rs` (select_cpu pin)
- `kernel/core/src/device/tty/serial.rs` (callback + BSP-pinned poller)
- `kernel/comps/uart/src/arch/aarch64/pl011.rs` (flush ICR; output pacing)
- `kernel/core/src/vm/vmar/vmar_impls/map.rs` (exec-map stall site)
- `kernel/core/src/process/program_loader/elf/load_elf.rs` (ELF load)
- Artifacts: `target/osdk/asterinas/asterinas-osdk-bin.qemu_elf`,
  `/tmp/asterina-rNN.img`, `/mnt/d/pi_sd/asterina.img` (currently R82)
