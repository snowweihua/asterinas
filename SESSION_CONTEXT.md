# SESSION_CONTEXT.md — RPi3 HW Bring-up (aarch64_support_pure)

## Objective
Get the RPi3B port merge-ready per `MERGE_PLAN.md`: hardware boots to an
interactive `/ #` shell with working console RX, then land the MR-1 arch
payload. (Superseded objective: reproduce/fix the old `[EL1-SYNC]` crash —
resolved in R47-R96.)

## STATUS (2026-10-05): RPi3 PL011 RX RESOLVED ON HARDWARE
`echo SHELL-ALIVE-42` is received, echoed, and **executed** on hardware
(`SHELL-ALIVE-42` output), 6/6 probes. Board boots cleanly to `/ #` with no
hang and no NUL flood.

**L3 re-validation**: 3/3 consecutive power-cycle boots reached `/ #` with no
hang and executed the probe (boot 3 had the bench-side stuck-byte flood
appended). QEMU `raspi3b` regression PASSES (`echo QEMU-RX-OK` →
`QEMU-RX-OK`).

### Root cause (two bugs masking each other)
1. `UartConsole::trigger_input_callbacks()` (`kernel/comps/uart/src/console.rs`)
   drained in an **unbounded `loop`**. A stuck PL011 RX status bit ("not
   empty") makes `recv()` keep returning a full buffer, so the loop spins
   forever in the IRQ handler and the tick poller. This is the true cause of
   the "RPi3B hangs without the gate" symptom (`eef289f82`).
2. The `rx_line_sane()` GPIO gate added to stop that hang then **masked RX**,
   blocking all input (R103/R104).

### Fix (aligned with the `aarch64_support` IRQ-based RX architecture)
- `kernel/comps/uart/src/console.rs`: bound the drain to 4×16 B (IRQ re-fires
  for the remainder).
- `kernel/comps/uart/src/arch/aarch64/pl011.rs`: `flush()` unmasks `RXIM`
  only (RTIM storms on an idle FIFO); no GPIO gate; add a timer-tick drain
  calling `reenable_rx_irq()` + the same `trigger_input_callbacks()` as the
  IRQ handler (VideoCore can clobber `ENABLE_IRQS_2`; handler-side re-assert
  alone is chicken-and-egg). The tick drain is gated to `is_rpi3()` (QEMU's
  IRQ path is reliable).
- `ostd/src/arch/aarch64/serial.rs`: drop `rx_line_sane()`; RPi3 re-asserts
  the IC routing only (no IMSC write from the tick).
- `aarch64_support` reference: `kernel/src/driver/mod.rs` — `IrqLine::alloc_
  specific(serial::irq_num())` + `on_active(uart_irq_handler)`,
  `init_rx_irq()` (unmask RXIM), `reenable_rx_irq()` (IC routing), plus
  `timer::register_callback_on_cpu(poll_uart_input)` sharing the drain.

### Backup (before the reference-based restore)
- Branch `backup-session-20261005` (commit `8ba5aec69`) — pre-restore session
  state (R105 diagnostics + IC-IoMem experiment).
- `/tmp/opencode/backup-20261005/` — file copies.

## Environment / workflow
- Dev in WSL2 (`/home/snow/asterinas`); SD card at `/mnt/d/pi_sd/`.
- Relay host `snow@10.142.15.37` (Pi 3B+, BT disabled per R101) reads
  `/dev/serial0` via `rxcap3.py` (logs `/home/snow/rx.log`, sends 6 probes).
- Build (Docker `asterinas/aarch64-dev:latest`):
  `cargo install --path osdk --force` + `cargo osdk build --release
  --target-arch aarch64 --boot-method qemu-direct --scheme aarch64-rpi3`,
  with `-e RUSTUP_TOOLCHAIN=nightly-2026-07-21-x86_64-unknown-linux-gnu`
  (otherwise rustup re-syncs and can hang ~15 min).
- ELF: `target/osdk/asterinas/asterinas-osdk-bin.qemu_elf`; convert with the
  explicit `source_elf` (MCP default points at the wrong `aster-nix` path).
- Deploy `/tmp/asterina-*.img` → `/mnt/d/pi_sd/asterina.img`, power-cycle,
  read `rx.log` on the relay.

## Hard constraints
- "Skipping is not fixing, just avoiding. Don't treat skipping as fixing!"
- Merge locked: 3-MR split; `test/rpi3/`, `specs/`, probes, `.github/`, `nix/`,
  `usr/`, `etc/`, agent scaffolding, `tools/*_mcp`, `pf_test/`, `reasonix.toml`
  are local-only; upstream restructure `kernel/src/*` → `kernel/core/src/*`.
- `unsafe` confined to `ostd/`; `kernel/` stays safe Rust.
- HW serial is lossy/fragmented (bursty drops); paced output survives.

## Branch / base
- Branch: `aarch64_support_pure`; base `4d395b885` (pre-arm).
- Pre-session HEAD: `eef289f82` (gate-restored, RX dead).
- Session commits: `bcc973db9` — RX fix; `b8831b7af` — gate the tick drain to
  `is_rpi3()`; `73c4b8ab3`/`011c64958` — status docs.

## Bench-side caveats (not kernel defects)
- The shell's own TX echo can loop back into RX on this bench (double echo)
  and the relay drops bytes; R101/L2.3 already document this.
- Register reads via the linear map can show store-forwarding/cache artifacts
  (`CR=IMSC=RIS=0x301`, `MIS≠RIS&IMSC`, `FR=0x0`); do not trust them as
  device state.

## Next moves (priority order)
1. Commit the RX fix (docs + 3 files) on `aarch64_support_pure`.
2. Strip any remaining local-only scaffolding; keep the tree MR-ready.
3. L2.3 residual: RX-input robustness — the bench TX→RX loop is out of scope;
   confirm the bounded drain + tick poller are the intended MR-1 shape.
4. L4: MR-2 generic bundle (item-by-item, x86-first) per MERGE_PLAN §5/§10.
5. L5 (deferred): upstream push + MR-1/MR-2 once the upstream author approves.

## Relevant files
- `kernel/comps/uart/src/console.rs` — bounded `trigger_input_callbacks`
- `kernel/comps/uart/src/arch/aarch64/pl011.rs` — flush RXIM-only, tick drain
- `ostd/src/arch/aarch64/serial.rs` — `reenable_rx_irq`, `has_data`/`receive`
- `ostd/src/arch/aarch64/bcm2836_irq.rs` — `reenable_uart_irq` (DSB/ISB)
- `ostd/src/arch/aarch64/timer/mod.rs` — tick re-assert call site
- `MERGE_PLAN.md` §11 — RX resolution log
- `.debug-journal.md` — R95-R105 history
