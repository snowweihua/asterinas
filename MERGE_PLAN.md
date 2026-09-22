# ARM64 Upstream Merge Plan (`aarch64_support` → `asterinas/asterinas`)

Status: APPROVED 2026-09-09. Branch base: `eb9edbd` ("Bump the Docker image to 0.16.1").
This document is the workbook for the merge; it stays local-only (never ported upstream).

## 1. Goal & success criteria

Port ~1 year of ARM64 bring-up into upstream-reviewable shape.
Done when: (a) all functional changes are arch-scoped or separately-justified
generic fixes; (b) x86_64 (+ riscv/loongarch) build and boot unchanged;
(c) zero environment junk in any MR diff.

## 2. Locked decisions (user, 2026-09-09)

1. **3-MR split**: MR-1 arch support · MR-2 generic SMP-correctness bundle · no MR-3.
2. **Local-only, never ports**: `test/rpi3/`, `specs/`, probe sources,
   `.github/` session logs, `nix/`, `usr/`, `etc/`, agent scaffolding
   (`.opencode`, `.specify`, `.roleflow`, `knowledge/`), `tools/*_mcp`,
   `pf_test/`, `reasonix.toml`.
3. **Fresh-branch port** from current upstream main (no history surgery).
   Push to fork (`origin`), MRs via fork.
4. **miniz `-O1` pin**: attempt proof; default to DROP.
5. Open: clean-branch name (placeholder `aarch64-support-clean`);
   MR-2 stacked vs separate (proposed: separate, after MR-1 stable).

## 3. Baseline recon (eb9edbd..HEAD: 482 commits, 0 merges, 100% ours)

| Bucket | Size | Disposition |
|---|---|---|
| Junk (never ports) | ~1000 files | `nix/store` (923), `usr/bin/busybox`, `etc/*`, `sbin`, `pf_test/`, `test_swap.s`, agent dirs, session logs, `tools/*_mcp` |
| Arch payload (MR-1 core) | 39 files, +6.7K, ~0 deletions | 3 arch dirs + `kernel/src/syscall/arch/aarch64.rs` (new, 352 lines; follows pre-existing upstream `x86/riscv/loongarch.rs` convention — arch code lives in 4 places) |
| OSDK enablers (MR-1) | ~15 files | `aarch64-rpi3.ld.template` (new), `aarch64.ld.template`, `base_crate/mod.rs`, `bundle/*`, `commands/build/*`, `config/scheme/boot.rs` |
| Other-arch stubs | 9 files, additive | `kernel_physical_base` / `frame_paddr_base` / `current_user_page_table_paddr` on x86/riscv/loongarch — REQUIRED companions (generic code calls them); travel with MR-2 |
| Generic ostd fixes | ~40 files | mm/frame+heap, sync workarounds, boot/smp, cpu/task/timer, page_table (see §5) |
| Generic kernel fixes | ~70 files | PSCI reboot, signals, execve, UART driver, stat layout + small-fix tail (see §5) |
| Root configs | 3 files | `rust-toolchain.toml` (dropped x86 target — must widen to both), `Cargo.toml` (miniz pin + dropped `codegen-units = 1`), `OSDK.toml` (stale virt scheme, minimal rpi3 scheme, `init_args` change) |
| Whitespace noise | 39 markers | Trailing-newline-only diffs — bulk-revert |
| Debug artifacts (must go) | 4 files | `device/mod.rs` heap test, `ramfs/fs.rs` HashMap test, `time/softirq.rs` empty `if`s, `waitid.rs` info! logs |

Notes:
- Base is ~1 year old; upstream reachable (HEAD `414f2770` at recon time).
- `osdk/deps/{frame,heap}-allocator` are SEPARATE copies of `ostd/src/mm/{frame,heap}`
  (not symlinks); fixes are mirrored in both — port both identically.
- `kernel/src/arch/aarch64`, `kernel/comps/pci/src/arch/aarch64`,
  `ostd/src/arch/aarch64` = the three main arch homes.

## 4. Strategy: fresh-branch port, not in-place cleanup

Rationale: junk never ports (no deletion commits needed); reviewable units;
base currency (a year of upstream movement absorbed once, up front);
squash-merge would hide the 482-commit history either way.

## 5. Disposition ledger

### MR-1 — arch support (port verbatim, then validate)
- `ostd/src/arch/aarch64/`, `kernel/src/arch/aarch64/`,
  `kernel/comps/pci/src/arch/aarch64/`, `kernel/src/syscall/arch/aarch64.rs`
- OSDK: `aarch64-rpi3.ld.template`, `aarch64.ld.template`, `base_crate/mod.rs`,
  `bundle/*`, `commands/build/*`, `config/scheme/boot.rs`
- Root (reconciled): both toolchain targets; `codegen-units = 1` restored;
  complete rpi3 `qemu.args`; delete stale `[scheme."aarch64"]`; revert `init_args`
- Bulk-revert all 39 trailing-newline-only files

### MR-2 — generic SMP-correctness bundle (one item at a time, x86-first)
- ostd: slab-list unique IDs; heap/frame bootstrap conversion series;
  atomic refcounts/mutex/inc_count; wfe/sev AP wait (assess genericity);
  `memory_region`/`kspace`/`frame/allocator+meta` base-offset plumbing
- ostd other-arch stubs required by the above callers
- kernel: PSCI reboot path; signal/execve arch glue is MR-1, but
  shebang fix, `kill` EPERM semantics, dentry caching, `vm/util` frame-dup,
  `getdents64` fix, n_tty stack buffer, `work_queue` split, PCI/virtio leniency,
  goldfish null — each with x86 boot validation
- RPi3-workaround cfg-gate audit (~20 sites): gate behavior-visible ones
  `#[cfg(target_arch = "aarch64")]`, keep provably-equivalent ones w/ comments

### DROP (never port; verify absent by grep on clean branch)
- All §2 junk; logger filtering/color changes; `core2→core3` migration;
  4 debug-artifact files; keyboard cosmetic line; `osdk/.../rust-toolchain.toml.template`
  `init_args` tweak; `osdk/src/config/mod.rs` 2-line unknown (re-derive or drop)

### NEEDS-REVIEW during P0
- miniz `-O1` (prove or drop) · `codegen-units` removal rationale (none found → restore)
- `kernel/src/vm/vmar/*` QEMU-TLB workaround + `clear_root_vmar` refactor (arch or generic?)
- `kernel/src/sched/sched_class` RPi3 single-core path (still active now APs schedule?)
- `kernel/src/device/tty/mod.rs` export; `kernel/src/process/*timer_manager*`,
  `thread/mod.rs`, `thread/task.rs` micro-edits (probably drop)

## 6. Phase plan with gates

| Phase | Work | Gate |
|---|---|---|
| P0 | Fetch upstream main; diff each MR-2 candidate (drop already-fixed, flag conflicts); miniz proof/drop; root-config reconciliation | Triaged list + miniz verdict |
| P1 | Create clean branch; port MR-1 set; normalize newlines; three-arch `cargo check`; x86 boot | All-arch check green, x86 unchanged |
| P2 | QEMU `raspi3b` + HW boot to prompt + auto-test on ported tree | Today's bar, both targets |
| P3 | MR-2 item-by-item with x86-first validation + cfg-gate audit | Per-item x86 + ARM boots |
| P4 | Push to fork; open MR-1, then MR-2; junk-absence grep | Review-ready |

## 7. Risks & unknowns

- P0 conflict volume (year of upstream `mm`/`sync` drift) — biggest unknown.
- Per-site cfg-gate audit (~20 sites) — biggest judgment load.
- `osdk/deps` ↔ `ostd` mirror discipline; propose upstream dedup as MR follow-up.
- Upstream CI / push-rights verification (fork push first).

## 8. Log

- 2026-09-09: plan approved (3-MR split, local-only set, port strategy, fork path,
  miniz default-drop). Recon: 482 commits / 1325 files characterized.
  Upstream HEAD at recon: `414f2770`. P0 starts on branch creation.
- 2026-09-09 P0 interim: upstream fetched (HEAD `414f2770`, tags 0.17.0/v0.16.2).
  Upstream ALREADY has distinct slab-list IDs (AtomicU64 allocator) → our list-ID
  fix DROPPED from MR-2. Heavy upstream churn in MR-2 candidate files
  (dentry −515, load_elf −548, kill −212, work_queue −204, msix −191, rwlock
  −254, page_table 116/105) → per-file re-derivation required, not mechanical
  porting. `arc_single.rs` is our new file (absent upstream).
- 2026-09-09 P0 update: upstream RESTRUCTURED (`kernel/src/*` →
  `kernel/core/src/*`, TTY split into device/driver/file/flags/hvc files).
  Consequence: MR-2 kernel items need relocation + re-derivation each
  (kill_all moved out of process/kill.rs; n_tty equivalent TBD; shebang region
  TBD). Porting = re-apply intent onto new code, file by file.
- 2026-09-09 P0.4 verdicts: DROPPED (upstream has it): list-ID fix, kill EPERM
  (upstream tolerant broadcast), shebang (upstream appends script path),
  getdents64 DirentInner, msix (upstream Result redesign), virtio leniency,
  goldfish (find_compatible+? rewrite). DROP (cosmetic): vm/util hunk.
  KEEP re-derive: work_queue start_workers split (upstream init identical to
  pre-fix); n_tty stack buffer → serial.rs + hvc.rs callbacks (both still
  heap-alloc upstream). Dentry: compare read-first against restructured
  lookup path at port time.
- 2026-09-09 P0.3 decisions (apply in P1 on clean branch): rust-toolchain
  targets = ["x86_64-unknown-none", "aarch64-unknown-none-softfloat"];
  restore `codegen-units = 1`; revert new-template init_args to ["sh","-l"];
  delete stale [scheme."aarch64"]; complete rpi3 qemu.args (draft: raspi3b,
  cortex-a53, smp 4, 1G, DTB path TBD — repo-relative preferred over
   /mnt/d; kernel+initrd+append console=ttyAMA0); miniz pin → P0.2 verdict.
- 2026-09-09 P0.2 verdict: miniz pin REMOVED (`Cargo.toml` `-O1` override
  deleted); unpinned image (3612144 bytes) boots HW clean to prompt +
  auto-test, `busybox nproc` = 4, QEMU smoke passes → DROP confirmed.
  Strict proof: kernel gunzips the initramfs in-kernel every boot
  (`rootfs.rs` `GZipDecoder`, magic `1F 8B`); this boot inflated the
  4682240-byte gzip to the full 44MB rootfs on HW with zero hangs —
  exercises `inflate_core` across the whole stream, the exact site of
  the old `-O3` hang. DROP proven, not just boot-clean.
- 2026-09-09 aarch64-rpi3 link fix: `.ap_boot` moved from low LMA to high
  VMA (mirrors `aarch64.ld`) with `__ap_boot_start/end`; fixes
  `R_AARCH64_ADR_PREL_PG_HI21 out of range` from `.text`. Root cause was
  original to `48511f36b` (low stub + missing symbols: rpi3 scheme never
  linked clean; prior HW images came from the generic scheme). Verified:
  fresh-OSDK build links, QEMU smoke passes, HW PSCI SMP 4/4 (`nproc`=4).
- 2026-09-15 (aarch64_support_pure, HEAD `cb4eefe48`): port of the 3-MR
  payload onto the pre-arm base now BOOTS TO A SHELL under QEMU `raspi3b`
  (initramfs unpack → `/init` AUTO-TEST script → `/bin/sh` → `/ #` prompt).
  Key functional fixes landed (candidates for MR-1/MR-2):
  1. **RPi3 virtio skip** (`kernel/core/comps/virtio/src/lib.rs`): no virtio
     devices on RPi3; network DmaPool allocation stalled boot during
     component init (APs not yet ready for IPI-driven TLB flushes).
     → MR-2 candidate: fix the real IPI/TLB-flush ordering instead of the
     skip; the skip itself is arch-gated and must not ship as-is.
  2. **Entropy/vsock registry leniency** (`device/entropy/mod.rs`,
     `device/socket/mod.rs`): `COMPONENT.get()?` / `ENTROPY_DEVICE_TABLE.get()?`
     so `random::init()` / net vsock init don't unwrap-None on device-less
     platforms. MR-1/2 generic-fix candidate (needs x86 validation).
  3. **AArch64 EL1 user-page-fault routing** (`kernel/core/src/arch/aarch64/
     cpu.rs`): `PageFaultInfo::try_from` accepts `DataAbortCurrentEL` so EL1
     kernel accesses to user addresses use the VMAR handler instead of
     panicking. MR-1 (arch-scoped) candidate.
- 2026-09-15 open blockers on the RPi3 hardware (same clean image):
  - **Intermittent hang during initramfs unpacking** — page-cache write path
    (`ramfs write_at → VMO page_cache.write → collect_pages`) spins forever at
    varying entries; QEMU completes it. Suspected RPi3-specific race in
    xarray cursor / page-commit / frame-or-heap allocator refill. NOT resolved.
  - **Init-startup EL1 fault** when unpacking completes — panic
    `Cannot handle user page fault` at `mm/fault/mod.rs:85`,
    fault addr `0x7ffffefc9000` (init user-stack region), no ex_table
    recovery. NOT resolved (QEMU does not hit it).
  - Serial RX (PL011 input) unproven on both QEMU and HW.
  These must be resolved before P2 (HW boot to prompt + auto-test on the
  ported tree) can pass on the RPi3.
