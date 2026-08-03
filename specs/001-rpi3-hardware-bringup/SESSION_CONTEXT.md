# RPi3 Debug Session — 2026-08-03 (Session 17)

## Current Task
Fix RPi3 kernel boot crash during `kspace::init_kernel_page_table()` — Level 1 Address Size Fault when MMU walks page tables to access frame metadata at PA 0x2b7b000.

## Previous Session Findings (Sessions 14-16)

### DEFINITIVE FINDING: VBAR_EL1 Handler Works
The EL1 exception handler IS firing and printing register dumps. The crash is a real Level 1 Address Size Fault.

## Session 17 Progress — CRITICAL DISCOVERIES

### Critical Finding #1: Memory Layout
The boot page tables are located at different PAs than assumed:
- `boot_l3pt` = PA **0x85000** (NOT 0x83000)
- `boot_l2pt_gb0` = PA **0x86000** (NOT 0x87000)
- `boot_l3pt_linear` = PA **0x82000** (the ACTUAL root page table used at boot)

### Critical Finding #2: MMU Uses boot_l3pt_linear as Root
The slot0 code reads `boot_root_pa = current_page_table_paddr() = 0x82000` (boot_l3pt_linear).
The cursor walk goes through:
1. KPT[0x1c0] = 0x82003 → boot_l3pt_linear at PA 0x82000
2. boot_l3pt_linear[0] = 0x84003 → boot_l2pt at PA 0x84000
3. boot_l2pt[0x15] = 0x2b7b003 → frame metadata at PA 0x2b7b000

BUT boot_l3pt_linear[447] = 0 and [448] = 0 — these entries for boot_l2pt_gb0 were ZERO!

### Critical Finding #3: Root Frame is at 0x337b000
The root page table PA is NOT 0x82000. It's at 0x337b000. The KPT is allocated from the frame allocator and lives at PA 0x337b000.

The cursor code accesses the root via VA 0xffff80000337b000 (identity-mapped).

### Critical Finding #4: boot_l3pt_linear[0x1c0] vs [447,448]
- Metadata VA 0xffff_e000_0000_0000 → L0 index 511, L1 index 447, L2 index 0x1c0
- But the cursor log shows L0[0x1c0] = 0x82003 (boot_l3pt_linear as L0 table)
- This means L0[0x1c0] → boot_l3pt_linear[0] → boot_l2pt[0x15]

Wait — the VA being accessed is 0xffff_e000_0000_0000. The walk shows L0[0x1c0] = 0x82003. Let me re-examine...

Actually, looking at cursor output:
```
[cursor] level=0x04 idx=0x1c0 pt_paddr=0x337b000
[cursor] pte_is_present=0x1 pte_paddr=0x82000
[cursor] level=0x03 idx=0x0 pt_paddr=0x82000
[cursor] pte_is_present=0x1 pte_paddr=0x84000
```

Level 4 = L0. L0[0x1c0] = 0x82003 → points to L1 at PA 0x82000 (boot_l3pt_linear)
Level 3 = L1. L1[0] = 0x84003 → points to L2 at PA 0x84000 (boot_l2pt)
Level 2 = L2. L2[0x15] = 0x2b7b003 → points to frame at PA 0x2b7b000

The CRASH address is FAR=0x2b7d104. This is WITHIN the metadata frame (0x2b7b000-0x2b7bfff). The issue is that L2[0x15] entry gives PA 0x2b7b000, but the walk should continue to L3 for a 4KB page mapping. The entry 0x2b7b003 has bits[1:0]=11 (Table descriptor), meaning the MMU tries to use it as a table pointer, not a page descriptor. But the entry should have been 0x2b7b003 for a PAGE descriptor (bits = 0x3 for level 3 page).

Wait, looking more carefully:
- L2 entry format: bits[1:0] = 0b11 means Table descriptor (points to L3)
- For a block/page descriptor at L2, bits[1:0] should be 0b11 with Block descriptor type

Actually for 4KB granule:
- Level 2 block descriptor: bits[1:0] = 0b01 (Block)
- Level 3 page descriptor: bits[1:0] = 0b11 (Page)

0x2b7b003 = 0b0010_1011_0111_1011_0000_0000_0000_0011
bits[1:0] = 0b11 → Table descriptor!

But we want a PAGE descriptor at level 2? No — for 4KB granule with 39-bit VA, we have:
- L0: 9 bits (bits[47:39])
- L1: 9 bits (bits[38:30])  
- L2: 9 bits (bits[29:21])
- L3: 12 bits (bits[20:12]) — 4KB page

For a 4KB page at L2, you'd need to use L2 as a table entry (bit=1) to point to L3, then L3 has the page. OR use a Level 2 Block descriptor if you want 2MB blocks.

For metadata at PA 0x2b7b000, if we're using 4KB pages, we need:
L2[0x15] = TABLE descriptor (0x2b7b003) → points to L3 at PA 0x2b7b000
L3[0] = PAGE descriptor for offset 0

But L3 would be at PA 0x2b7b000, which IS the frame metadata itself! The frame metadata content starts at offset 0x1000 within that frame. So we'd be using the metadata frame itself as an L3 page table!

The cursor FAILS because when it tries to read boot_l2pt[0x15] = 0x2b7b003 and follow it as a table pointer to PA 0x2b7b000, the MMU sees that PA 0x2b7b000 doesn't have valid page table format (or the access causes another fault).

Actually wait, let me re-read the ESR:
ESR=0x96000035
- bits[31:26] = 0x25 = 100101 → Data Abort, Level 1
- bits[5:0] = 0x35 = 110101 → Address Size Fault

Level 1 Address Size Fault! This means the translation encountered a level where the output address exceeds the physical address space. At level 1 (L1 table), the table entry points to a PA that exceeds what a level 1 entry can address.

For ARMv8 with 4KB granule:
- Level 1 block descriptor can describe 1GB blocks
- Level 1 table descriptor points to L2 table

0x2b7b003 as a TABLE descriptor at L1: PA[47:12] = 0x2b7b0. This exceeds the range for a valid level 1 table pointer because bits above the physical address width are set... but we're on a 64-bit system with 40-bit or 48-bit PA space.

Actually, looking at the entry: 0x2b7b003 & ~0xFFF = 0x2b7b000. The MMU tries to read the next level page table at PA 0x2b7b000. If that frame is being used for metadata, it might not have the right format as a page table, causing the fault.

### Fixes Applied (This Session)

1. **Patched boot_l2pt_gb0[0x15]** = 0x2b7b003 via inline asm `str x1, [x0, x2]` — verified
2. **Patched boot_l3pt[0]** = 0x86003 (TABLE to boot_l2pt_gb0) — verified
3. **Patched boot_l3pt_linear[0x1c0]** = 0x2b7b403 (PAGE desc to metadata frame) — verified via read_volatile
4. **Patched boot_l3pt_linear[0x17f]** = 0x86003 (TABLE to boot_l2pt_gb0) — verified
5. **Patched boot_l3pt_linear[0x180]** = 0x86003 (TABLE to boot_l2pt_gb0) — verified

### Current Crash Still Occurs
```
ESR=0x96000035 ELR=ffff000000361ef0 FAR=0000000002b7d104
```

The cursor walk:
```
[cursor] level=4 idx=0x1c0 pt_paddr=0x337b000 (root frame)
[cursor] pte_is_present=1 pte_paddr=0x82000
[cursor] level=3 idx=0x0 pt_paddr=0x82000
[cursor] pte_is_present=1 pte_paddr=0x84000
[EL1-SYNC]
```

Still crashes at same place — after L1 table read at PA 0x82000.

### Key Mystery
The boot_l3pt_linear[0x17f] and [0x180] were showing 0 before patching. But these should be for L1 table entries that index into L2 tables. When we read L1[0] (at boot_l3pt_linear), we get 0x84003 which is L2 at 0x84000. So where does 0x84000 come from if [0x17f]=[0x180]=0?

Wait — I'm confusing indices. Let me trace again:
- VA 0xffff_e000_0000_0000
- L0 index = bits[47:39] = 511 = 0x1ff
- L1 index = bits[38:30] = 447 = 0x17f
- L2 index = bits[29:21] = 448 = 0x1c0 (because bits[38:30] = 0x17f means bits[29:21] of the remaining VA...)
- Actually let me recalculate for VA 0xffff_e000_0000_0000:
  - Binary: 1111_1111_1111_1111 _ 1110_0000_0000_0000 _ 0000_0000_0000_0000 _ 0000_0000_0000_0000
  - L0[47:39] = 0x1ff = 511
  - VA_bits[38:30] in the remaining 39 bits after removing prefix 0xffff_e...
  
Actually for 0xffff_e000_0000_0000:
If we strip the high bits 0xffff_e000_0000_0000:
VA[63:48] = 0xffff (all 1s for kernel high VA)
VA[47:39] = 0x1ff = 511
VA[38:30] = 0x17f = 447  
VA[29:21] = 0x000 = 0
VA[20:12] = 0x000 = 0
VA[11:0] = 0x000 = 0

So the walk should be:
- L0[511] → L1 at some PA
- L1[447] → L2 at some PA
- L2[0] → L3 or page

But cursor shows level=0x04 idx=0x1c0. 0x1c0 = 448 decimal! So L2 index is 448, not 0.

Hmm, maybe the VA is not exactly 0xffff_e000_0000_0000 but slightly different. Let me check what VA the cursor is actually walking.

## Files Modified This Session
- `ostd/src/mm/page_table/mod.rs` — Extensive page table fixup with debug output
- Debug output added: boot_l3pt_linear[0x17f], [0x180] checks; after_dsb checks; PATCH and verify messages

## Known Blockers
1. **Root cause unclear** — boot_l3pt_linear[0x17f]=[0x180]=0 were clearly wrong, but patching them to 0x86003 didn't fix the crash
2. **MMU walk still fails** — The L1 table read at PA 0x82000 still causes fault
3. **Possibility: TTBR1 might not be set to the expected root** — Need to verify TTBR1 value

## Session 18 Progress - Original Abort Fixed

### Exact Fault Mechanism Confirmed
- `ELR=0xffff000000361ef0` symbolizes to the `ldaxrb` in `PageTableNodeRef::lock()`.
- `FAR=0x2b7d104` is the lock byte in the metadata slot for page-table frame PA `0x84000`:
  - metadata base PA = `0x2b7b000`
  - frame index = `0x84000 / 0x1000 = 0x84`
  - slot = `0x2b7b000 + 0x84 * 64 = 0x2b7d100`
  - `PageTablePageMeta.lock` offset = 4, giving `0x2b7d104`
- Normal metadata reads/writes at this range work. The RPi3 faults specifically on the exclusive `ldaxrb/stxrb` lock operation during bootstrap.

### Toggle Proof
- Without the bootstrap lock bypass: reproducible `[EL1-SYNC]`, ESR `0x96000035`, ELR `0xffff000000361ef0`, FAR `0x2b7d104`.
- With `PageTableNodeRef::lock()` returning an unchecked guard while `IN_BOOTSTRAP_CONTEXT` is true: the board passes the exact faulting point and no exception is raised.

### Additional Correction
- The experimental overwrite of boot L2 entry `0x2a00705` with `0x2b7b003` was wrong. `0x2a00705` is the valid 2 MiB block mapping that covers the metadata PA. It is now preserved.
- For VA `0xffffe00000000000` with 48-bit, four-level translation, the software level-4 index is `0x1c0`; previous notes claiming L0 index `0x1ff` were incorrect.

### Current New Blocker
- Boot now advances past the former synchronous abort.
- It reaches the final subtree guard successfully (`lock` bypassed, `stray == false`) and then stalls before observable progress in `dfs_acquire_lock()`.
- This is a new later blocker, not the original Level 1 Address Size Fault.

## Next Action
1. Add markers immediately around `guard_level = subtree_root.level()`, `cur_node_va`, and the call to `dfs_acquire_lock()` in `lock_range()`.
2. If DFS is entered, print the level before calling `cur_node.entry(i)`; if not, inspect `level()` metadata and address arithmetic.
3. Keep the bootstrap-only lock bypass; do not restore exclusive atomics until after `IN_BOOTSTRAP_CONTEXT` becomes false.

## Debug Markers in Code
- `[npt] FIX: KPT[447/448]` — Fixing KPT entries to point to boot_l3pt_linear
- `[npt] Setting boot_l3pt_linear[0x17b]` — Direct frame mapping
- `[npt] Setting boot_l3pt[0]` — TABLE to boot_l2pt_gb0
- `[npt] Setting boot_l3pt_linear[0x1c0]` — PAGE to metadata
- `[npt] PATCH boot_l3pt_linear[0x17f/0x180]` — L1 table entries
- `[slot0] boot_root_pa=`, `[slot0] pte=`, `[slot0] l3table_pa=`, `[slot0] l3table[0]=`
- `[cursor] level=`, `idx=`, `pt_paddr=`, `pt_vptr=`, `pte_is_present=`, `pte_paddr=`, `next_pt_addr=`
- `[kspace.m0/m1/m2/m3]` — metadata init stages
- `[akt.0b]` — TTBR1 value dump
- `[akt.0c/d/e]` — L0[0], L0[256], L0[511] dumps
- `[akt.0f/g/h]` — L1[0], L1[511], L1[448] dumps
