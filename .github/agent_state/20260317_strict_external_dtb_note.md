# 2026-03-17 strict external DTB note

## Goal
- Remove embedded DTB fallback for AArch64 boot and keep boot working with an external DTB only.

## Functional fixes kept
- `OSDK.toml`: for scheme `aarch64`, QEMU now also loads `test/nix/aarch64-virt.dtb` at a fixed RAM address via `-device loader,file=test/nix/aarch64-virt.dtb,addr=0x48000000,force-raw=on`.
- `ostd/src/arch/aarch64/boot/mod.rs`:
  - Added deterministic DTB probe at `0x48000000` and retained RAM-scan fallback with stronger validation.
  - Removed embedded DTB fallback; boot now fatals if no external DTB is available.
  - Corrected AArch64 parsed memory regions by:
    - clipping DTB-derived usable and reserved ranges to the QEMU RAM window,
    - reserving the low pre-kernel loader gap `0x40000000..kernel_start`,
    - keeping the DTB region reserved.
- `ostd/src/boot/memory_region.rs`:
  - Fixed `MemoryRegion::kernel()` on AArch64 to return absolute DRAM physical addresses (`0x40000000 + offset`) instead of low offsets.
- `ostd/src/mm/frame/meta.rs`:
  - `get_slot()` on AArch64 now uses raw physical metadata-slot pointers during `IN_BOOTSTRAP_CONTEXT`, then switches to linear-mapping access later.
  - Bootstrap metadata marking is now unified back to the common `Segment::from_unused` flow.

## Validation
- Clean build log: `target/agent_logs/20260317_strict_clean_build.txt`
- Clean run log: `target/agent_logs/20260317_strict_clean_run.txt`
- Result: `EQ`, `RUN_EXIT=0`

## Follow-up status
- `arch::enable_cpu_features()` has been re-enabled on AArch64 and is stable.
- `mm::kspace::activate_kernel_page_table()` has been re-enabled on AArch64 and is stable.
- Latest validation with metadata-path unification: `target/agent_logs/20260317_meta_unify_run.txt` (`EQ`, `RUN_EXIT=0`).

## Latest hardening update (no DTB RAM scan)
- `ostd/src/arch/aarch64/boot/mod.rs`:
  - Removed RAM-scan fallback for DTB discovery.
  - DTB source is now deterministic only: prefer register-provided DTB when valid, otherwise use fixed loader DTB address `0x48000000` when valid.
  - Validation check now consistently requires an FDT with both `/memory` and `/cpus` nodes.
- Validation logs:
  - Build: `target/agent_logs/20260317_strict_dtb_noscan_build.txt`
  - Run: `target/agent_logs/20260317_strict_dtb_noscan_run.txt`
  - Result: `EQ`

## Cleanup update (temporary boot markers removed)
- `ostd/src/arch/aarch64/boot/mod.rs`:
  - Removed temporary early UART markers (`[1]`, `[2]`, `[3]`) from `aarch64_boot()`.
  - Kept fatal-path message (`[3x] FATAL: no DTB source available`) unchanged.
- Validation logs:
  - Build: `target/agent_logs/20260317_remove_boot_markers_build.txt`
  - Run: `target/agent_logs/20260317_remove_boot_markers_run.txt`
  - Result: `EQ`

## Cleanup update (remove unused helper)
- `ostd/src/arch/aarch64/boot/mod.rs`:
  - Removed unused `pl011_putc` helper.
  - Kept `pl011_puts_static` because it is used by kernel/ostd marker paths (including `K0/K1/K2/M/EQ`).
- Validation logs:
  - Build: `target/agent_logs/20260317_remove_pl011_putc_build.txt`
  - Run: `target/agent_logs/20260317_remove_pl011_putc_run.txt`
  - Result: `EQ`

## Cleanup update (extern comment warning)
- `ostd/src/arch/aarch64/boot/mod.rs`:
  - Converted the `pl011_puts_asm` extern-block comment from doc style (`///`) to plain comment (`//`) to avoid `unused_doc_comments` warning.
- Validation logs:
  - Build: `target/agent_logs/20260317_extern_comment_cleanup_build.txt`
  - Run: `target/agent_logs/20260317_extern_comment_cleanup_run.txt`
  - Result: `EQ`

## Cleanup update (remove phase-I UART markers)
- Removed remaining debug marker emissions based on `pl011_puts*`:
  - `kernel/src/lib.rs`: removed `K0/K1/K2` markers.
  - `ostd/libs/ostd-macros/src/lib.rs`: removed `M` marker from generated `__ostd_main` paths.
  - `ostd/src/arch/aarch64/qemu.rs`: removed `EQ` marker from `exit_qemu`.
  - `ostd/src/arch/aarch64/boot/mod.rs`: removed now-unused `pl011_puts_static` helper.
- Validation logs:
  - Build: `target/agent_logs/20260317_remove_phase1_uart_markers_build.txt`
  - Run: `target/agent_logs/20260317_remove_phase1_uart_markers_run.txt`
  - Result: boot reaches `exit_qemu` path and command returns successfully (no marker prints expected).

## Architecture-boundary cleanup update
- Reduced arch leakage from generic boot code:
  - `ostd/src/boot/memory_region.rs` no longer uses `#[cfg(target_arch = "aarch64")]` for kernel base computation.
  - Added per-arch `kernel_physical_base(kernel_start, kernel_loaded_offset)` in:
    - `ostd/src/arch/aarch64/boot/mod.rs`
    - `ostd/src/arch/x86/boot/mod.rs`
    - `ostd/src/arch/riscv/boot/mod.rs`
    - `ostd/src/arch/loongarch/boot/mod.rs`
- Validation logs:
  - Build: `target/agent_logs/20260317_arch_boundary_kernel_base_build.txt`
  - Run: `target/agent_logs/20260317_arch_boundary_kernel_base_run.txt`
  - Result: runtime remains successful.

## Architecture-boundary cleanup update (frame metadata base)
- Reduced AArch64 hardcoded base handling inside generic frame metadata code:
  - Added `frame_paddr_base()` in arch mm modules:
    - `ostd/src/arch/aarch64/mm/mod.rs` returns `0x4000_0000`
    - `ostd/src/arch/x86/mm/mod.rs`, `ostd/src/arch/riscv/mm/mod.rs`, `ostd/src/arch/loongarch/mm/mod.rs` return `0`
  - `ostd/src/mm/frame/meta.rs` now uses `crate::arch::mm::frame_paddr_base()` in:
    - metadata slot address mapping (`frame_to_meta`/`meta_to_frame`)
    - bounds checks and frame index calculations in `get_slot`
    - total frame count computation and unusable-range clipping
- Validation logs:
  - Build: `target/agent_logs/20260317_arch_boundary_meta_base_build.txt`
  - Run: `target/agent_logs/20260317_arch_boundary_meta_base_run.txt`
  - Result: runtime remains successful.

## Architecture-boundary cleanup update (meta bootstrap path genericized)
- Further reduced architecture-specific branches in `ostd/src/mm/frame/meta.rs`:
  - Replaced AArch64-only slot lookup `#[cfg]` branches in `get_slot` with generic runtime logic keyed by `arch::mm::frame_paddr_base()`.
  - Unified slot initialization via shared `init_slots(...)` helper.
  - Replaced AArch64-only frame-count and unusable-range branches with base-aware generic logic.
  - Replaced AArch64-only metadata-base static with generic `FRAME_META_PADDR_BASE` used only when non-zero frame base architectures require it.
- Validation logs:
  - Build: `target/agent_logs/20260317_meta_arch_boundary_bootstrap_path_build.txt`
  - Run: `target/agent_logs/20260317_meta_arch_boundary_bootstrap_path_run.txt`
  - Result: runtime remains successful.

## Final boundary audit snapshot
- Remaining non-arch-file diffs are now mostly architecture-neutral cleanups or generic calls into arch APIs:
  - `ostd/src/boot/memory_region.rs`: uses `crate::arch::boot::kernel_physical_base(...)` instead of local arch branches.
  - `ostd/src/mm/frame/meta.rs`: generic logic keyed by `arch::mm::frame_paddr_base()`; no explicit `#[cfg(target_arch = "aarch64")]` branches remain in the refactored bootstrap slot path.
  - `ostd/src/lib.rs`, `ostd/src/boot/mod.rs`, `kernel/src/lib.rs`, `ostd/libs/ostd-macros/src/lib.rs`: cleanup/removal of temporary bring-up markers and stage skips.
- Remaining architecture-specific behavior lives in arch modules (`ostd/src/arch/*`) through helper APIs.
