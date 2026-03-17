# AArch64 Phase-1 Recovery Note (2026-03-13)

## Summary
- Goal: push AArch64 phase-1 boot past the post-`[5h]` handoff and reach kernel entry or clean QEMU exit.
- Current status: restored stable baseline; latest verified runtime markers are `[1] [2] [3] [3x] [3a] [4] [4b] [5h] J` and then timeout.
- Primary blocker: execution reaches the byte immediately before the post-boot transfer, but no first-instruction marker from any callee appears.

## Last verified behavior
- Build command: `cargo osdk build --scheme aarch64 --target-arch aarch64`
- Run command: `timeout 30s cargo osdk run --scheme aarch64 --target-arch aarch64 > target/agent_logs/20260313_retry21_restored_baseline_with_M_run.txt 2>&1; echo "RUN_EXIT=$?" >> target/agent_logs/20260313_retry21_restored_baseline_with_M_run.txt`
- Key output markers observed: `AMVRB [1] [2] [3] [3x] [3a] [4] [4b] [5h] J`, then `qemu-system-aarch64: terminating on signal 15`, `RUN_EXIT=124`

## Files touched
- `ostd/src/arch/aarch64/boot/mod.rs`
- `ostd/libs/ostd-macros/src/lib.rs`

## What was tested in this step
- Added a safe macro-generated entry marker `M` at the first instruction of `__ostd_main`; `M` never appeared.
- Switched `__ostd_main` declarations/definitions to `extern "C"`; no change.
- Replaced `__ostd_main` with a local never-return probe that should print `L`; `L` never appeared.
- Removed the explicit `sp` reset before the post-`J` transfer; no change.
- Reverted transient probes and restored the stable direct handoff path.

## New verified findings after disassembly
- Disassembly of the final ELF showed the original Rust-side `mov sp, boot_stack_top` sequence wrote scratch stack slots onto the physical bytes of `__ostd_main` before the `bl`, because `boot_stack_top` resolved to `0x000c6000` and `__ostd_main` started at physical `0x000c6018`.
- `boot.S` was also computing a bogus "virtual" stack by adding `KERNEL_VMA` to the low `.boot.stack` symbol `boot_stack_top`; that address aliases into the start of `.text`, not into a real high-half stack mapping.
- Fixed both issues by removing the Rust-side stack reset in `ostd/src/arch/aarch64/boot/mod.rs` and keeping the identity-mapped boot stack in `ostd/src/arch/aarch64/boot/boot.S`.
- After switching the `M` probe to a static-slice UART helper, the runtime advanced to `... J M K0`.

## Current boundary
- Last coherent marker sequence: `[1] [2] [3] [3x] [3a] [4] [4b] [5h] J M K0`.
- Interpretation: control now reaches `__ostd_main` and enters `kernel::main()`.
- The next stall is inside the top of `kernel::main()` after the first direct UART marker.

## Conclusion from latest probes
- The original post-`J` hang was caused by stack alias/corruption across the handoff path.
- That handoff bug is fixed enough to reach `__ostd_main` and `kernel::main()`.
- The next blocker is later: after `K0` at the top of `kernel::main()`, before subsequent direct UART markers or clean PSCI shutdown.

## Next concrete step
- Disassemble the current `_ZN9aster_nix4main...` body and confirm why execution stalls between the first and second direct UART calls (`K0` -> `K1`), then reduce that path further around `exit_qemu()` or the second UART call.

## If this session crashes
- Resume from: `boot.S` identity-stack fix applied, Rust-side stack reset removed, static `M` probe kept, kernel `early_println!` skipped on AArch64 with `K0/K1/K2` direct markers in place.
- First command to run: `timeout 30s cargo osdk run --scheme aarch64 --target-arch aarch64 > target/agent_logs/20260313_retry25_kernel_markers_run.txt 2>&1; echo "RUN_EXIT=$?" >> target/agent_logs/20260313_retry25_kernel_markers_run.txt`

## Canonical Handoff Files
- Current checkpoint pointer: `.github/agent_state/CURRENT_CHECKPOINT.md`
- Next-step pointer: `.github/agent_state/NEXT_STEP.md`
- Latest-log pointer: `.github/agent_state/LATEST_LOG.md`
- Minimal restart prompt: `.github/agent_state/RESUME_PROMPT.md`

## Delta update (2026-03-13, resumed)

### New work completed
- Reproduced the current coherent boundary again with fresh build/run logs:
	- Build: `target/agent_logs/20260313_retry30_revert_putc_build.txt`
	- Run: `target/agent_logs/20260313_retry30_revert_putc_run.txt`
- Verified marker sequence remains: `AMVRB [1] [2] [3] [3x] [3a] [4] [4b] [5h] J M K0`, then timeout (`RUN_EXIT=124`).
- Disassembled current `kernel::main` and confirmed compiled order is: `K0` print -> `K1` print -> `init()` -> `K2` print -> `exit_qemu()` call.
- Disassembled current `ostd::arch::qemu::exit_qemu` and confirmed it now emits `EQ`, then executes `hvc #0` directly (no `log::debug!` path in front).

### Important findings from this resumed pass
- One attempted probe (`KA` / `putc` between `K0` and `K1`) intermittently perturbed earlier marker visibility (regressed back to `[5h]`-only boundary in transient runs), so it was reverted.
- After probe reversion, the stable/reproducible boundary returned to `J M K0`.
- Therefore, no new stable forward progress beyond `K0` yet; the blocker remains in the very top of `kernel::main` path before visible `K1`/`K2`/`EQ` output.

### Environment recovery note
- `/tmp/arm-gic-patched` was missing again in this resumed session; it was recreated from cached `arm-gic-0.7.1` and re-patched (`is_multiple_of` -> modulo checks) so AArch64 builds run again.

### Next concrete step (updated)
- Keep source as the reverted stable state and perform the next probe in a way that does not perturb early boot layout (e.g., isolate by disassembling + symbol/addr validation first, then a minimal non-structural probe around the second `pl011_puts_asm` call site in `kernel::main`).

## Delta update (2026-03-13, continued)

### New work completed
- Disassembled `__ostd_main` and confirmed it also uses a small fixed `0x80` stack frame before calling `kernel::main()`.
- Verified the current symbol layout still keeps the identity-mapped boot stack below the early text:
	- `boot_stack_top = 0x00000000000c6000`
	- `__ostd_main = 0xffff0000000c787c`
	- `kernel::main = 0xffff0000000c78e4`
- Confirmed from the saved runtime tail that the stable build really completes the full `K0\n` write before timing out; the stall is after the first direct UART call returns or immediately after it.

### Probe attempted and outcome
- Tried a minimal control-flow probe by calling `exit_qemu()` immediately after `K0` in `kernel::main`.
- That source change was not stable: the resulting runtime regressed all the way back to pre-handoff markers, stopping before `J`/`M`/`K0`.
	- Probe log: `target/agent_logs/20260313_retry31_post_k0_exit_probe_run.txt`
- Reverted that probe and re-ran the restored baseline.
	- Restored baseline log: `target/agent_logs/20260313_retry32_restore_after_post_k0_probe_run.txt`
- After reversion, the coherent boundary returned to `AMVRB [1] [2] [3] [3x] [3a] [4] [4b] [5h] J M K0` with timeout (`RUN_EXIT=124`).

### Updated interpretation
- The current source remains layout-sensitive enough that even a small control-flow edit near `K0` can perturb much earlier behavior and produce misleading regressions.
- The stable/reproducible boundary is still `J M K0`; no trustworthy forward progress beyond `K0` was established in this pass.
- The remaining high-value next step should avoid changing early layout, favoring binary-only inspection, symbol/address validation, or a probe that changes literals without materially changing control flow.

## Delta update (2026-03-13, continued 2)

### New work completed
- Ran a literal-only probe that reused the first marker literal for the second call site in `kernel::main` (no control-flow change).
	- Probe log: `target/agent_logs/20260313_retry33_k1_literal_alias_probe_run.txt`
- Probe result still stalled after a single marker line (`... J M K0`), so the failure is not tied to the original second literal address alone.

### Important correction discovered
- During probe/revert churn, `kernel::main` marker literals were accidentally swapped in source (`K1` first, `K0` second).
- This explained a transient runtime sequence `... J M K1` in reconfirmation logs and did not indicate true forward progress.
	- Transient log showing swapped-order effect: `target/agent_logs/20260313_retry35_reconfirm_stable_after_alias_probe_run.txt`

### Restored stable state
- Restored intended marker order in source (`K0` first, `K1` second).
- Re-ran baseline and re-confirmed coherent boundary returned to `... J M K0` with timeout.
	- Restored-order log: `target/agent_logs/20260313_retry36_marker_order_restored_run.txt`

### Updated interpretation
- Literal-only aliasing of the second marker did not move the boundary beyond the first marker call.
- The blocker remains on the path immediately after the first marker call in `kernel::main`.
- Current source has been restored to the intended stable marker order; no transient probe state remains.

## Delta update (2026-03-13, continued 3)

### Probe attempted
- Tried a stronger isolation probe by removing the first marker call so `K1` became the first call in `kernel::main`.
	- Probe log: `target/agent_logs/20260313_retry37_k1_as_first_call_probe_run.txt`

### Outcome
- This edit was not minimally perturbing in practice: runtime regressed to a pre-handoff boundary (stopped before `J`/`M`), so the result is not usable for root-cause inference.
- Reverted immediately and re-ran baseline.
	- Restore log: `target/agent_logs/20260313_retry38_restore_after_first_call_probe_run.txt`
- After restore, coherent boundary returned to `... J M K0` with `RUN_EXIT=124`.

### Updated guidance
- In this build state, even removing one early marker call can invalidate comparability by shifting layout too much.
- Prefer binary-only inspection and symbol/addr validation over source-level control-flow edits around the `K0`/`K1` region.

## Delta update (2026-03-13, continued 4)

### Binary-only revalidation (no source edits)
- Re-collected current symbol addresses from the active ELF:
	- `boot_stack_top = 0x00000000000c6000`
	- `__ostd_main = 0xffff0000000c787c`
	- `kernel::main = 0xffff0000000c78e4`
	- `exit_qemu = 0xffff0000001d32d4`
	- `pl011_puts_asm = 0xffff0000001e6fa8`
- Re-disassembled `__ostd_main`, `kernel::main`, `exit_qemu`, and `pl011_puts_asm`; compiled `kernel::main` path is still:
	- first marker call at `0xffff0000000c7910` (`K0` literal at base+`0x608`)
	- second marker call at `0xffff0000000c7930` (`K1` literal at base+`0x60b`)
	- third marker call at `0xffff0000000c7954` (`K2` literal at base+`0x60e`)
	- `exit_qemu` call at `0xffff0000000c7968`

### GDB usefulness assessment
- GDB is useful here as a targeted tool (single-step after `K0`), but only after low-perturbation prep.
- High-value breakpoints/watch points for a future GDB pass:
	- `*0xffff0000000c7914` (instruction immediately after first `pl011_puts_asm` returns)
	- `*0xffff0000000c7930` (second marker call site)
	- `*0xffff0000001e6fb4` (PL011 FR polling loop)
	- `*0xffff0000001d3300` (`exit_qemu` marker write)

### Updated interpretation
- Binary shape remains consistent with prior findings; no evidence of source drift in the critical path.
- Next step can stay non-invasive: a GDB single-step pass at the listed PCs to determine whether control is stuck in UART FR polling versus never reaching the second call site.

## Delta update (2026-03-16, resumed)

### New work completed
- Recreated missing `/tmp/arm-gic-patched` (again) from cached `arm-gic-0.7.1` and re-applied the `is_multiple_of` -> modulo compatibility patch so AArch64 builds run.
- Ran multiple non-invasive GDB passes with `gdb-multiarch` using OSDK QEMU GDB stub (`--gdb-server wait-client,addr=.osdk-gdb-socket`):
	- `target/agent_logs/20260316_gdb_break_trace3_multiarch.txt`
	- `target/agent_logs/20260316_gdb_break_trace4_conditional.txt`
	- `target/agent_logs/20260316_gdb_break_trace5_return_or_loop.txt`
	- `target/agent_logs/20260316_gdb_break_trace6_post_init.txt`
- Captured corresponding runtime logs:
	- `target/agent_logs/20260316_gdb_server_wait_client_run3.txt`
	- `target/agent_logs/20260316_gdb_server_wait_client_run4.txt`
	- `target/agent_logs/20260316_gdb_server_wait_client_run5.txt`

### Key verified findings
- Execution reaches and returns from the second marker call path:
	- hits at `0xffff0000000c7930` (second `pl011_puts_asm` call site), then `0xffff0000000c7934` (return site).
- Execution also reaches the post-`init()` path and the third marker call site:
	- hit at `0xffff0000000c7938` (after `init()` call)
	- hit at `0xffff0000000c7954` (third `pl011_puts_asm` call site)
- Despite this, runtime logs still only show `J`, `M`, `K0` in normal output.

### Updated interpretation
- The old assumption “hang strictly before second call site” is now disproven.
- Under debugger control, control flow progresses through second call return and up to third call site; the observed missing `K1`/`K2` output is therefore likely tied to argument/state behavior at call time or UART write behavior, not an outright control-flow stop at `K0`.
- Next non-invasive step: inspect live argument values/memory around `sp` slot at `0xffff0000000c7914` and `0xffff0000000c7930` (where length is reloaded) and add a targeted watchpoint strategy to identify what mutates that slot between first and later calls.

## Delta update (2026-03-16, continued)

### New work completed
- Ran focused GDB value traces for `x0/x1/sp` and stack slot inspection:
	- `target/agent_logs/20260316_gdb_break_trace7_values.txt`
	- `target/agent_logs/20260316_gdb_break_trace8_setup_microtrace.txt`
	- `target/agent_logs/20260316_gdb_break_trace9_single_bp_7930.txt`
	- `target/agent_logs/20260316_gdb_break_trace11_watch_fixed.txt`
- Confirmed with a single-breakpoint control run (`0xc7930` only) that `x1 == 0` at second call site, so this is not an artifact of earlier breakpoint stops.
- Confirmed runtime output still ends at `J M K0` in corresponding runs (`...run6`/`...run8`/`...run10`).

### Strong signal from traces
- At `0xc7930` and `0xc7954`, `x1` is consistently `0` and `*sp` observed as `0`, while `x0` points to correct marker bytes (`K1\\n`, `K2\\n`).
- In microtrace, `x1` becomes `3` before first call setup, but stack slot observation remains `0` at probe points; net effect is later marker calls execute with zero length.

### Additional source probe and outcome
- Tested a minimal source probe to emit `K1` via three `pl011_putc` calls (to avoid `pl011_puts_static` length path):
	- Probe log: `target/agent_logs/20260316_retry39_k1_putc_probe_run.txt`
- Result: still no visible `K1`; runtime remained `J M K0`.
- Reverted the probe and reconfirmed baseline:
	- `target/agent_logs/20260316_retry40_restore_after_k1_putc_probe_run.txt`

### Updated interpretation
- Missing `K1/K2` is not explained solely by `pl011_puts_static` length reload mechanics.
- There is likely a debugger-sensitive/timing-sensitive behavior: GDB can step through later PCs, but free-running runtime output remains capped at `K0`.
- Next step should minimize Heisenbug effects while capturing state transitions between `0xc7910` and `0xc7930` (e.g., lightweight instrumentation in `pl011_puts_asm` path or controlled QEMU tracing), keeping code perturbation minimal and reversible.

## Delta update (2026-03-16, continued 2)

### New work completed
- Ran low-perturbation QEMU traces (no breakpoints) via `--qemu-args`:
	- `target/agent_logs/20260316_retry41_qemu_trace.log` (+ run log `...retry41_qemu_trace_run.txt`)
	- `target/agent_logs/20260316_retry42_qemu_trace_cpu.log` (+ run log `...retry42_qemu_trace_cpu_run.txt`)
- Correlated trace PCs with marker/runtime behavior while preserving baseline source state.

### Key verified findings from CPU trace
- `pl011_puts_asm` entries by caller return address (`x30`) show:
	- first kernel marker call (`x30=0xc7914`) enters with `x1=3` (expected `K0`).
	- second marker call (`x30=0xc7934`) enters with `x1=0`.
	- third marker call (`x30=0xc7958`) enters with `x1=0`.
- Trace also confirms control reaches `exit_qemu` function entry (`PC=0x1d32d4`), but no observed `pl011_puts_asm` entry with `x30=0x1d3304` in this free-running trace window.

### Supporting targeted GDB step
- Single-step from `exit_qemu` entry (`0x1d32d4`) advances through early prologue instructions, then becomes unstable around `0x1d32ec` in stepped context (`target/agent_logs/20260316_gdb_break_trace12_exitq_step.txt`), reinforcing that behavior differs between stepped and free-running paths.

### Updated interpretation
- Free-running evidence now strongly indicates argument collapse to zero length specifically for K1/K2 call entries (`x1=0`), while K0 remains valid (`x1=3`).
- The issue boundary is refined to state corruption/invalid state propagation between first and second call setup, plus uncertain behavior around early `exit_qemu` body in free-running mode.

## Delta update (2026-03-16, continued 3)

### New work completed
- Fixed OSDK CLI invocation issue for QEMU argument injection:
	- invalid form confirmed: `-- --qemu-args "..."`
	- valid form used: repeated `--qemu-args=` tokens (`-d`, `in_asm,exec,cpu`, `-D`, `<trace_file>`)
- Captured a fresh low-perturbation trace run:
	- `target/agent_logs/20260316_retry44_qemu_trace_cpu.log`
	- `target/agent_logs/20260316_retry44_qemu_trace_cpu_run.txt`

### Key verified findings (reproducibility)
- Runtime markers remain capped at `J M K0`.
- Trace includes both `exit_qemu` entry and call site:
	- `PC=0x1d32d4` present
	- `PC=0x1d3300` present
- `pl011_puts_asm` entry caller/value mapping reproduces prior result:
	- `x30=0xc7914 -> x1=3` (K0)
	- `x30=0xc7934 -> x1=0` (K1 path)
	- `x30=0xc7958 -> x1=0` (K2 path)
- Still no observed `pl011_puts_asm` entry with `x30=0x1d3304` in this run.

### Updated interpretation
- The `x1` collapse for K1/K2 is now reproduced with corrected trace injection syntax; it is not an artifact of the earlier failed `retry43` command.
- `exit_qemu` decode reaches its call-site address, but entry-level caller evidence into `pl011_puts_asm` from `x30=0x1d3304` remains absent in this capture.

## Delta update (2026-03-16, continued 4)

### New work completed
- Ran another low-perturbation CPU trace (`retry45`) with corrected split `--qemu-args` form:
	- `target/agent_logs/20260316_retry45_qemu_trace_cpu.log`
	- `target/agent_logs/20260316_retry45_qemu_trace_cpu_run.txt`
- Ran finer-grained single-step trace (`retry46`) to inspect instruction-level state around `0xc7900 -> 0xc7930`:
	- `target/agent_logs/20260316_retry46_singlestep_trace.log`
	- `target/agent_logs/20260316_retry46_singlestep_trace_run.txt`
- Attempted stack-slot hardware watchpoint passes (`retry49/retry50`) via `gdb-multiarch`; sessions were inconsistent due socket race/attach timing and did not yield stable watchpoint trigger data.

### Key verified findings
- `retry45` reproduces invariant again:
	- `x30=0xc7914 -> x1=3`
	- `x30=0xc7934 -> x1=0`
	- `x30=0xc7958 -> x1=0`
- `retry45` still shows decode reachability at `0x1d3300`, while no observed `pl011_puts_asm` entry with `x30=0x1d3304`.
- `retry46` instruction-level sequence between first call and return shows only `pl011_puts_asm` PCs (no detour/exception path in this window), and confirms:
	- at `0xc7900`: `x1=3`
	- at `0xc7910` (call): `x1=3`
	- at `0xc7914` (post-return): `x1=0` (callee clobber expected)
	- at `0xc7918` (after `ldr x1, [sp]`): still `x1=0`
	- at `0xc7930` (second call): `x1=0`

### Updated interpretation
- Collapse is now localized more tightly: `ldr x1, [sp]` in caller reloads `0`, even though caller stored `3` to `[sp]` before the first call and no alternate control-flow is seen in this call window under single-step trace.
- This points to stack-slot value loss/overwrite semantics rather than just register clobber by callee.

## Delta update (2026-03-16, continued 5)

### New work completed
- Added singlestep trace with interrupt logging (`retry51`):
	- `target/agent_logs/20260316_retry51_singlestep_int_trace.log`
	- `target/agent_logs/20260316_retry51_singlestep_int_trace_run.txt`

### Key verified findings
- Runtime markers still end at `J M K0`.
- Critical transition PCs are reached in order with expected values:
	- `0xc7910` with `x1=3`
	- `0xc7914` with `x1=0`
	- `0xc7918` with `x1=0`
	- `0xc7930` with `x1=0`
- First logged exception (`Taking exception 3 [Prefetch Abort]`) appears at line `44170`, i.e. **after** the critical K0→K1 transition window.
- Last PC before first exception is around `0x1d32e8` (inside `exit_qemu` path), so these exceptions do not explain the earlier `x1` collapse.

### Updated interpretation
- Interrupt/exception activity captured by `-d int` is downstream and not causal for the `x1` reload collapse in `kernel::main` transition window.
- Root cause remains a stack-slot value loss/overwrite before/at `ldr x1, [sp]` without an observed control-flow detour in that window.

## Delta update (2026-03-16, continued 6)

### New work completed
- Ran direct GDB memory-check pass with corrected escaped register expressions (`retry53`):
	- `target/agent_logs/20260316_retry53_memcheck_gdb.txt`
	- `target/agent_logs/20260316_retry53_memcheck_run.txt`

### Key verified findings
- At breakpoint `0xc7900` (before `str x1, [sp]`):
	- `x1=3`, `sp=0xc5d50`, and `*[sp]=0x0`.
- Single-step one instruction to `0xc7904` (after executing `str x1, [sp]`):
	- `x1` remains `3`, but `*[sp]` is still `0x0`.
- At breakpoint `0xc7914` (after first `pl011_puts_asm` returns):
	- `x1=0` and `*[sp]` still `0x0`.
- This is a direct observation that the caller stack store at `0xc7900` does not materialize in memory under this phase/path.

### Updated interpretation
- Leading hypothesis is upgraded: this is not just register clobber or later overwrite; stack writes at this stage/path appear ineffective (or not visible) for the inspected slot.
- This directly explains why `ldr x1, [sp]` reloads `0` and why K1/K2 calls enter with zero length.

## Delta update (2026-03-16, resolution)

### Final root cause confirmation
- QEMU `monitor info mtree` in `target/agent_logs/20260316_retry56_mtree_gdb.txt` proved the guest memory map is:
	- flash/ROM at `0x00000000-0x03ffffff`
	- RAM at `0x40000000-0x5fffffff`
- The linked AArch64 image was still using `KERNEL_LMA = 0x80000`, which placed both the early boot code and `.boot.stack` in the flash region.
- The failing stack slot observed in GDB (`sp = 0xc5d50`) is therefore physically inside flash, not DRAM.
- That fully explains the earlier evidence:
	- `str x1, [sp]` at `0xc7900` never changed memory
	- `ldr x1, [sp]` reloaded `0`
	- K1/K2 calls entered `pl011_puts_asm` with `x1 = 0`

### Fix implemented
- Updated `osdk/src/base_crate/aarch64.ld.template` to use `KERNEL_LMA = 0x40080000` so the linked image is loaded into DRAM on QEMU `virt`.
- Updated `ostd/src/arch/aarch64/boot/boot.S` so:
	- `TTBR0_EL1` keeps the identity mapping for the low physical execution path
	- `TTBR1_EL1` uses a dedicated `boot_l4pt_kern` root
	- the high-half kernel VA range maps to DRAM starting at physical `0x40000000`
	- `sp` continues to use `boot_stack_top`, which now resolves into DRAM because of the corrected LMA

### Verification
- Rebuilt with:
	- `cargo osdk build --scheme aarch64 --target-arch aarch64`
- Verified with:
	- `timeout 40 cargo osdk run --scheme aarch64 --target-arch aarch64 > target/agent_logs/20260316_fix_verify_run.txt 2>&1; echo "RUN_EXIT=$?" >> target/agent_logs/20260316_fix_verify_run.txt`
- Final runtime markers are now:
	- `AMVRB [1] [2] [3] [3x] [3a] [4] [4b] [5h] J M K0 K1 K2 EQ`
	- `RUN_EXIT=0`

### Conclusion
- The phase-1 blocker is resolved.
- The real bug was physical placement, not a mysterious stack corruption inside the code path itself.
- Any future AArch64 bring-up on QEMU `virt` should keep the boot image and boot stack inside the DRAM aperture beginning at `0x40000000`.

## Delta update (2026-03-16, continued restoration)

### Goal
- Start restoring AArch64 from phase-1 debug bypasses toward normal OSTD boot flow while keeping the LMA/DRAM root-cause fix intact.

### What changed
- Restored AArch64 boot to repopulate `EARLY_INFO` and use `call_ostd_main()` again, instead of direct `__ostd_main` jump.
- Removed the unconditional early return/direct jump from `ostd::init` so control flow is standard again.
- Added no-DTB fallback path in `ostd/src/arch/aarch64/boot/mod.rs` for OSDK `qemu-direct` runs where `device_tree_paddr == 0`.
	- Verified by GDB: `device_tree_paddr=0` at `aarch64_boot` and memory at `0x40000000` was zeros in this run mode.
- Localized init stall to `mm::frame::meta::alloc_meta_frames` and then fixed early slot writes to use identity-mapped PA on AArch64 before linear mapping setup.
- Reduced fallback memory window to 8 MiB for no-DTB bring-up speed so metadata bootstrap can complete within debug timeout.

### Key verification logs
- `target/agent_logs/20260316_continue_restore_run.txt` (restored path initially stalled around `Fdt::from_ptr`)
- `target/agent_logs/20260316_continue_restore_gdb2.txt` (proved `device_tree_paddr=0`)
- `target/agent_logs/20260316_continue_identity_slots_run.txt` (slot loop progressed after identity-map write fix)
- `target/agent_logs/20260316_continue_fallback_8m_run.txt` (restored flow reaches `EQ`, `RUN_EXIT=0`)

### Current status
- AArch64 restored flow is no longer limited to the old direct handoff bypass; with no-DTB fallback active, it reaches clean `EQ` exit again.
- Current no-DTB fallback mode is intentionally constrained (8 MiB usable region) for fast bring-up iteration; it is not final production memory discovery.

## Delta update (2026-03-16, DTB restoration)

### New work completed
- Corrected AArch64 boot-register interpretation in Rust entry signature (`x0` treated as DTB pointer; `x1` reserved).
- Added ARM64 Linux Image header at `_start` in `ostd/src/arch/aarch64/boot/boot.S` (magic `0x644d5241` at offset `0x38`), verified in ELF bytes.
- Wired explicit QEMU DTB file in AArch64 scheme (`-dtb test/nix/aarch64-virt.dtb`) and generated that DTB via QEMU `dumpdtb`.
- Implemented DTB discovery logic in `ostd/src/arch/aarch64/boot/mod.rs`:
	- direct register handoff path (`device_tree_paddr != 0`),
	- RAM scan discovery when register handoff is absent,
	- DTB parse via `fdt::from_ptr` and normal `EARLY_INFO` population.

### Verified outcome
- `target/agent_logs/20260316_dtb_embedded_run.txt` now shows:
	- `[3x] DTB discovered by RAM scan`
	- `[3x] before fdt::from_ptr`
	- `[4] after device_tree once`
- This confirms AArch64 boot re-enters DTB-driven initialization path instead of the old hardcoded no-DTB path.

### Current boundary / remaining blocker
- With full DTB memory map active, short timeout runs spend long time in frame metadata initialization and may not reach `EQ` within the current timeout budget.
- Hardcoded fallback boot-info path remains as last-resort safety path only.

## Delta update (2026-03-16, DTB boot completion)

### New work completed
- Removed the hardcoded non-DTB boot-info fallback path from the normal AArch64 bring-up path.
- Reserved the live DTB blob in parsed memory regions so `early_alloc()` no longer allocates metadata pages on top of the DTB at low DRAM.
- Added AArch64-specific identity-mapped access for early boot page-table frame manipulation in `ostd/src/mm/page_table/boot_pt.rs`.
- Bypassed the boot-page-table DFS firmware-marking walk on AArch64 bring-up and added TLB invalidation after metadata mappings are inserted.
- Introduced AArch64 metadata tracking base offset (`0x40000000`) and an 8 MiB bring-up cap so metadata init only covers the active DRAM aperture needed for current debug runs.

### Verified outcome
- `target/agent_logs/20260316_dtb_final_clean_run.txt` reaches:
	- `[3x] DTB discovered by RAM scan`
	- `[3x] before fdt::from_ptr`
	- `[4] after device_tree once`
	- `EQ`
	- `RUN_EXIT=0`

### Root-cause notes
- One blocker was allocator collision with the live DTB blob at low DRAM; reserving the DTB region fixed the first-meta-page stall.
- Another blocker was using `paddr_to_vaddr()` for early AArch64 boot page-table frame accesses before linear mapping existed; identity-mapped physical access fixed the boot PT mapping stall.