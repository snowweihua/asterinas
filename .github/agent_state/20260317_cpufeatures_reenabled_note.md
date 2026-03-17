# 2026-03-17 AArch64 CPU features re-enabled

## Scope
- Continued from strict external-DTB stable baseline.
- Re-enabled BSP `arch::enable_cpu_features()` on AArch64 in `ostd/src/lib.rs`.

## Key finding
- `enable_cpu_features()` now executes successfully in early init when the strict-DTB/bootstrap metadata fixes are present.
- The previous hang no longer reproduces in this baseline.

## Code status
- `ostd/src/lib.rs`: BSP now calls `arch::enable_cpu_features()` on AArch64 again.
- `ostd/src/arch/aarch64/mod.rs`: kept a post-CPACR barrier (`isb`) after setting `CPACR_EL1::FPEN::TrapNothing`.
- Temporary `[cf*]` probe markers were removed.

## Validation
- Build: `target/agent_logs/20260317_cpufeat_clean_build.txt`
- Run: `target/agent_logs/20260317_cpufeat_clean_run.txt`
- Result: `EQ`, `RUN_EXIT=0`
