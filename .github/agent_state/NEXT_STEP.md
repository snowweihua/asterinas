# Next Step — 2026-03-25

## Immediate Problem
`tlbi vmalle1` hangs in QEMU 6.2 AArch64 TCG when DAIF.I=0 (IRQs enabled).
Current IRQ-mask fix in `tlb_flush_all_excluding_global` causes 120-line regression (hang at initramfs).

## Fix Strategy

### Step 1: Probe 120-line hang
Add probe to `tlb_flush_all_excluding_global` to confirm:
1. Is it even being called during initramfs? (print something before `dsb ish`)
2. Does `dsb ish` hang after IRQ mask? Or is the hang somewhere else?

### Step 2: If dsb ish hangs with IRQs masked
Try `dsb sy` (system-wide) or `dsb nsh` (non-shareable) instead.

### Step 3: If fix is correct, find the other source of the 120-line hang
It may be `tlb_flush_addr` calling `tlbi vaae1` for kernel VAs — that might also be hanging.
Add a probe to `tlb_flush_addr` kernel-VA branch too.

### Step 4: Once tlbi works, verify page fault loop is fixed
The repeated 300K+ READ faults at 0x52c078 (DFSC=0x7, L3 translation fault) were caused by TTBR0 toggle
losing QEMU's walk cache for existing user mappings. With `tlbi vmalle1`, the walk cache is properly
invalidated and this should no longer repeat.

## Key Files
- `ostd/src/arch/aarch64/mm/mod.rs` — TLB flush functions (MAIN TARGET)
- `kernel/src/vm/vmar/vm_mapping.rs` — page fault handlers (has TlbFlushOp::for_all() calls + probes)
- `kernel/src/thread/exception.rs` — page fault dispatcher (has [pf] probes)
- `ostd/src/arch/aarch64/cpu/context.rs` — enable_local() before UserException return
- `ostd/src/mm/tlb.rs` — dispatch_tlb_flush (calls disable_local THEN flush_all)

## Build Command
```bash
cargo build --target aarch64-unknown-none-softfloat -p aster-nix 2>&1 | tail -3
```

## Run Command
```bash
rm -f /tmp/qemu_serial.log && timeout 120 cargo osdk run --scheme aarch64 --target-arch aarch64 > /dev/null 2>&1
wc -l /tmp/qemu_serial.log && grep -E "\[pf\]|\[around\]|\[tlbi\]|\[spf\]" /tmp/qemu_serial.log | head -30
```

## Known Workarounds That Work/Don't Work
| Approach | Result |
|---|---|
| TTBR0 toggle in `tlb_flush_all_excluding_global` | Works for kernel TLB, works for new user mappings, but BREAKS QEMU walk cache for existing user mappings |
| Plain `tlbi vmalle1` (no IRQ mask) | Hangs at 251 lines: `tlbi` itself hangs with DAIF.I=0 |
| `tlbi vmalle1` with `daifset #3` save/restore | Hangs at 120 lines: regression, new issue during initramfs |
| `tlbi vaae1, {va}` for user VAs in `tlb_flush_addr` | Hangs (was already disabled) |

## Ultimate Goal
Get busybox `sh` prompt on AArch64 QEMU 6.2.
