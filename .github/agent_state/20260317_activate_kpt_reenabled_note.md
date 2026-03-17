# 2026-03-17 AArch64 activate_kernel_page_table re-enabled

## Scope
- Re-enabled BSP `mm::kspace::activate_kernel_page_table()` on AArch64 in `ostd/src/lib.rs`.

## Validation
- Build: `target/agent_logs/20260317_activate_kpt_build.txt`
- Run: `target/agent_logs/20260317_activate_kpt_run.txt`
- Result: `EQ`, `RUN_EXIT=0`

## Current AArch64 status
- Strict external DTB path remains active and stable.
- `enable_cpu_features()` is enabled and stable.
- `activate_kernel_page_table()` is now enabled and stable.
