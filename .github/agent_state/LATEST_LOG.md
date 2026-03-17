# Latest Log Pointer

- Latest successful verification runtime: `target/agent_logs/20260316_fix_verify_run.txt`
- Latest root-cause proof log: `target/agent_logs/20260316_retry56_mtree_gdb.txt`
- Latest decisive memory-check log: `target/agent_logs/20260316_retry53_memcheck_gdb.txt`
- Latest key observation: QEMU `virt` exposes RAM only at `0x40000000-0x5fffffff`; the previous boot stack at `0xc5d50` lived in flash, so stack stores could not persist.
- Final verified outcome: runtime now reaches `J M K0 K1 K2 EQ` and exits with `RUN_EXIT=0` after moving `KERNEL_LMA` to DRAM and fixing the TTBR1 high-half mapping.

- Latest restoration run (standard flow resumed): `target/agent_logs/20260316_continue_restore_run.txt`
- Latest DTB contract proof (no DTB in qemu-direct run): `target/agent_logs/20260316_continue_restore_gdb2.txt`
- Latest metadata-loop progress run: `target/agent_logs/20260316_continue_identity_slots_run.txt`
- Latest fallback fast-path success: `target/agent_logs/20260316_continue_fallback_8m_run.txt`
- Latest DTB-driven path run: `target/agent_logs/20260316_dtb_embedded_run.txt`
- Latest DTB scan attempt run: `target/agent_logs/20260316_dtb_scan2_run.txt`
- Latest key observation: with `-dtb test/nix/aarch64-virt.dtb`, qemu-direct still does not hand DTB via register on this setup; AArch64 boot now discovers DTB by RAM scan and successfully enters `fdt::from_ptr` path.
- Latest clean DTB-driven verification run: `target/agent_logs/20260316_dtb_final_clean_run.txt`
- Latest decisive fix set: reserve DTB blob in parsed memory regions, use identity-mapped access for early AArch64 boot page-table manipulation, and cap tracked RAM to 8 MiB during bring-up.