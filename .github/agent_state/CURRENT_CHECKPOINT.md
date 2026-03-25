# Current Checkpoint

**Date:** 2026-03-25

## Status
- Build: PASSING (`cargo build --target aarch64-unknown-none-softfloat -p aster-nix`)
- Run: STUCK at 120 lines — hangs at "[kernel] unpacking the initramfs.cpio.gz to rootfs ..."
  - REGRESSION introduced by IRQ-masking fix in `tlb_flush_all_excluding_global`
  - Before regression: hung at 251 lines (after first page fault around-maps, `tlbi vmalle1` with IRQs enabled)
  - Root cause of original hang: `tlbi vmalle1` hangs when DAIF.I=0 (IRQs ENABLED) in QEMU 6.2 TCG

## Work Done (cumulative — complete Phase II + Phase III)

### All Sync Primitive Fixes (Phase II)
- `ostd/src/mm/page_table/node/mod.rs`: lock() uses `swap(1, Acquire)` spinloop
- `ostd/src/mm/frame/meta.rs`: `get_from_unused` uses retry loop on spurious CAS failure
- `ostd/src/sync/rcu/mod.rs`: compare_exchange retry loop when observed == expected
- `ostd/src/sync/rwmutex.rs`, `rwlock.rs`, `mutex.rs`, `spin.rs`: fetch_or/swap patterns
- `ostd/src/task/processor.rs`: `before_switching_to` uses `swap(true, AcqRel)`

### Phase III: Page Fault Handling
- `kernel/src/vm/vmar/vm_mapping.rs`: Added `TlbFlushOp::for_all().perform_on_current()` after cursor.map() in both `handle_page_faults_around` and `handle_single_page_fault`
- Many `early_println!` debug probes still present: `[pf]`, `[hpf]`, `[around]`, `[spf]`, `[bst]`, `[kte]`, `[ast]`, `[mp]`, `[rnt]`, `[st2]`, `[tlbi]` etc.

### TLB Flush Investigation (Phase III Key Findings)
1. Confirmed with `mrs daif` probe: `tlbi vmalle1` hangs when DAIF.I=0 (IRQs ENABLED)
2. Confirmed: `tlbi vmalle1` works fine when DAIF.I=1 (IRQs disabled, e.g. during early boot)
3. `tlb_flush_all_including_global`: called once during early boot with DAIF=0x3c0 (all masked) → works
4. `tlb_flush_all_excluding_global`: called from page fault handler with DAIF=0x300 (I=0, IRQs enabled) → hangs
5. TTBR0 toggle workaround: worked for new mappings but caused QEMU to lose walk cache for EXISTING user page mappings → AT S1E0R fails with PAR=0x80f (L3 translation fault) for already-mapped pages

### Current Code State: `ostd/src/arch/aarch64/mm/mod.rs`
- `tlb_flush_all_excluding_global`: saves DAIF, masks with `daifset #3`, does tlbi+dsb+isb, restores DAIF
  - This was intended to fix the IRQs-enabled hang, but introduces a 120-line regression
- `tlb_flush_all_including_global`: plain `tlbi vmalle1` (called only once at boot with IRQs disabled)
- `tlb_flush_addr`: skips TLBI for user VAs (workaround), uses `tlbi vaae1` for kernel VAs

### 120-line Regression Hypothesis
- The `msr daif, {saved_daif}` restore at end of `tlb_flush_all_excluding_global` may be prematurely
  re-enabling IRQs during initramfs (kernel-space ops call this via `dispatch_tlb_flush`)
- OR: `dsb ish` hangs when IRQs are masked (QEMU 6.2 bug: dsb ish waits for IRQ processing)
- OR: Timing issue — the fix works for the page fault path but breaks something else

## Boot Trace (most recent run with IRQ mask fix)
```
...scheduler context switches...
[kernel] unpacking the initramfs.cpio.gz to rootfs ...
<--- HANGS HERE (120 lines total) --->
```

## Boot Trace (run with plain tlbi vmalle1, no IRQ mask)
```
...scheduler context switches...
[kernel] unpacking the initramfs.cpio.gz to rootfs ...
...[pf] #0 addr=0x1401bcc0 esr=0x82000007 dfsc=0x7
[around] map va=0x14010000 ... (16 pages)
[tlbi] daif=0x300 I=0 pre-tlbi
<--- HANGS HERE (254 lines total, after first around-maps) --->
```

## Current Blocker
After `[kernel] OSTD initialized. Preparing components.`, the run enters an
infinite loop calling `alloc_frame_with` over and over (thousands of iterations,
no panic/hang/progress). No further `[kernel]` or component markers appear.

**Hypothesis:** Component initialization is triggering an unbounded memory
allocation loop — likely in a component's `init()` calling some setup that
allocates frames in a loop. Candidates to investigate:
- `kernel/comps/` component init order
- Any component init that creates large pre-allocated structures
- Whether the page-table activation path is somehow re-entered

## Persisted Artifacts
- Patched arm-gic crate: `target/agent_cache/arm-gic-patched/` (copied from `/tmp/arm-gic-patched`)
  - Restore to `/tmp`: `cp -r target/agent_cache/arm-gic-patched /tmp/arm-gic-patched`
  - The Cargo.toml in the kernel/ostd references this via `path = "/tmp/arm-gic-patched"` — restore before building.

## Latest Logs
- Build: `target/agent_logs/20260318_phase2_cleanup_build.txt`
- Run:   `target/agent_logs/20260318_phase2_cleanup_run.txt`

## Next Steps for Resume
1. Add finer-grained probe markers in `kernel/src/lib.rs` `kernel::main` to identify which component's init triggers the loop.
2. Alternatively, add a counter/limit to the alloc_frame probe to print a backtrace-style callsite after N calls.
3. Investigate `kernel/comps/` init functions for any that allocate large arrays of frames.

## Resume Instructions
- Read this file first.
- Read `.github/agent_state/2026-03-13-phase1-handoff-probe.md` for full Phase I context.
- Latest run log: `target/agent_logs/20260318_phase2_cleanup_run.txt`