# Current Checkpoint

- Canonical checkpoint file: `.github/agent_state/2026-03-13-phase1-handoff-probe.md`
- Current status: phase-1 root cause remains fixed, and AArch64 DTB-driven boot now reaches `EQ` with `RUN_EXIT=0` again.
- Current coherent boundary: real DTB path is active (`[3x] DTB discovered by RAM scan` -> `fdt::from_ptr` -> `EARLY_INFO` from DTB), verified by `target/agent_logs/20260316_dtb_final_clean_run.txt`.
- Current blocker: register DTB handoff is still absent in this `qemu-direct` path (`x0=0`), so boot currently relies on DTB RAM scan plus DTB-region reservation.
- Current workaround: AArch64 bring-up still caps tracked RAM to 8 MiB in frame metadata init for practical debug-time completion.
- Use this file as the first resume target in a new chat.