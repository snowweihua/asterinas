# Next Step

- Status: the phase-1 AArch64 boot blocker is resolved.
- Verified fix: `KERNEL_LMA` now points at DRAM (`0x40080000`) and TTBR1 uses a dedicated high-half page-table root that maps kernel virtual addresses to physical DRAM.
- Verified outcome: `target/agent_logs/20260316_fix_verify_run.txt` ends with `J M K0 K1 K2 EQ` and `RUN_EXIT=0`.

- Status update: DTB-driven AArch64 boot still reaches `EQ` with `RUN_EXIT=0`, and full-range metadata initialization no longer needs the temporary 8 MiB cap.
- Latest DTB-driven evidence: `target/agent_logs/20260317_phys_base_metadata_clean_run.txt` shows `[3x] DTB discovered by RAM scan`, `[3x] before fdt::from_ptr`, `[4] after device_tree once`, then `EQ` with `RUN_EXIT=0`.
- Current limitation: DTB is still discovered by RAM scan rather than register handoff, and several `ostd::init()` phases remain AArch64-gated for staged re-enable.

## If more AArch64 work resumes

- Keep using `cargo osdk build --scheme aarch64 --target-arch aarch64` and `cargo osdk run --scheme aarch64 --target-arch aarch64` so runs stay on the direct-boot scheme.
- If a future regression looks like “stores to boot stack do nothing”, check the QEMU physical memory map first and confirm the image LMA still sits inside the DRAM aperture starting at `0x40000000`.
- If early high-half execution regresses, validate TTBR1 against `boot_l4pt_kern` before instrumenting the Rust side again.
- Next concrete target: continue re-enabling skipped `ostd::init()` phases under DTB-driven flow (allocator init has been re-enabled), then switch DTB handoff from RAM scan fallback to register handoff where available.