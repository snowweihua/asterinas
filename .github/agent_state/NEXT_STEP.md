# Next Step

- Status: the phase-1 AArch64 boot blocker is resolved.
- Verified fix: `KERNEL_LMA` now points at DRAM (`0x40080000`) and TTBR1 uses a dedicated high-half page-table root that maps kernel virtual addresses to physical DRAM.
- Verified outcome: `target/agent_logs/20260316_fix_verify_run.txt` ends with `J M K0 K1 K2 EQ` and `RUN_EXIT=0`.

- Status update: DTB-driven AArch64 boot is now working end-to-end again. `target/agent_logs/20260316_dtb_final_clean_run.txt` reaches `EQ` with `RUN_EXIT=0`.
- Latest DTB-driven evidence: runtime shows `[3x] DTB discovered by RAM scan`, `[3x] before fdt::from_ptr`, `[4] after device_tree once`, then `EQ`.
- Current limitation: the AArch64 bring-up path still relies on an 8 MiB metadata tracking cap for practical completion time, and DTB is discovered by RAM scan rather than register handoff.

## If more AArch64 work resumes

- Keep using `cargo osdk build --scheme aarch64 --target-arch aarch64` and `cargo osdk run --scheme aarch64 --target-arch aarch64` so runs stay on the direct-boot scheme.
- If a future regression looks like “stores to boot stack do nothing”, check the QEMU physical memory map first and confirm the image LMA still sits inside the DRAM aperture starting at `0x40000000`.
- If early high-half execution regresses, validate TTBR1 against `boot_l4pt_kern` before instrumenting the Rust side again.
- Next concrete target: replace the temporary 8 MiB AArch64 metadata cap with a correct scalable solution, then continue re-enabling skipped `ostd::init()` phases under DTB-driven flow.