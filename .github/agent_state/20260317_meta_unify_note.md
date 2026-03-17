# 2026-03-17 AArch64 metadata-path unification

## Scope
- Removed the temporary AArch64-only direct bootstrap metadata marking path from `ostd/src/mm/frame/meta.rs`.
- Restored unified marking via `Segment::from_unused` for unusable ranges and metadata pages.

## Kept prerequisite
- `get_slot()` still uses raw metadata physical pointers during `IN_BOOTSTRAP_CONTEXT` on AArch64, then uses linear mapping afterwards.

## Validation
- Build: `target/agent_logs/20260317_meta_unify_build.txt`
- Run: `target/agent_logs/20260317_meta_unify_run.txt`
- Result: `EQ`, `RUN_EXIT=0`
