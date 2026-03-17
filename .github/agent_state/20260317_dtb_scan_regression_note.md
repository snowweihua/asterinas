# 2026-03-17 AArch64 early-boot regression note

## Symptom
- Boot stalled between `[4] after device_tree once` and `[5h] after early_info once`.
- Fine-grained markers showed hang specifically inside `parse_kernel_commandline()`.

## Root-cause direction
- RAM DTB scan was selecting a false-positive DTB pointer from kernel image bytes (`discovered=0x4025c6f8`).
- This became reproducible after binary layout growth while re-enabling init phases.

## Changes made
- In `ostd/src/arch/aarch64/boot/mod.rs`:
  - Added precise kernel physical range helper and excluded that range in DTB RAM scanner (`find_dtb_paddr_in_qemu_ram`).
  - Kept scan high-to-low preference and high-window dense scan first.
  - Added temporary diagnostics markers (`[4a]..[4g]`, `[mr1]..[mr7]`, `[dtb] ...`) to verify stage transitions.

## Current result
- `cargo osdk run --scheme aarch64 --target-arch aarch64` reaches `EQ` with `RUN_EXIT=0` again.
- Current run used embedded DTB fallback (`reg=0x0 discovered=0x0`), but no longer hangs in `parse_kernel_commandline()`.

## Next step
- If strict non-embedded DTB is required, refine scanner heuristics to positively identify QEMU-provided DTB and/or pass DTB in x0 reliably.
