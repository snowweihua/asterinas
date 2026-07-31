# RPi3 Debug Session — 2026-07-31 (Session 16)

## Current Task
Fix RPi3 kernel boot crash during `kspace::init_kernel_page_table()` — cursor crashes accessing root frame via linear mapping after KPT activation.

## Previous Session Findings (Sessions 14-15)

### DEFINITIVE FINDING: VBAR_EL1 Handler Works
The EL1 exception handler IS firing and printing register dumps. The crash is a real Level 1 Address Size Fault.

### Previous Crash Signature (Pre-Session 16)
```
ESR=0x96000035 (Level 1 Address Size Fault)
FAR=0x2c48ec4
ELR=ffff000000361d28
```

## Session 16 Progress

### Root Cause Identified
The frame metadata VA range 0xffffe00000000000 is accessed via metadata VA (not linear mapping). The page table walk goes through:
1. KPT[511] = 0 (not set!) → should point to boot_l3pt_linear
2. boot_l3pt_linear[0] → points to boot_l2pt_gb0 (PA 0x86000)
3. boot_l2pt_gb0[21] = 0 (not set!) → should map frame metadata PA 0x2b00000

### Fixes Applied (Partial)

1. **KPT[256]** — Fixed to point to boot_l3pt_linear (was 0x85003, correct)
2. **boot_l3pt_linear[0]** — Fixed to point to boot_l2pt_gb0 (was 0x86003 → now 0x84003)
3. **boot_l2pt_gb0[25]** — Fixed to map root PA 0x337b000 (was 0x3200705 → now 0x337b0403)
4. **KPT[448]** — Set to boot_l3pt_high (0x83003)
5. **KPT[511]** — Set to boot_l3pt_high (0x83003) 
6. **boot_l3pt_high[0]** — Set to new_l2 at 0x85000
7. **new_l2[21]** — Set to frame metadata entry (0x2b00803)

### Current Crash (Still Failing)
```
ESR=0x96000035 (Level 1 Address Size Fault)
FAR=0000000002b7d144
ELR=ffff000000361ef0

cursor walk:
- KPT[511]=0x0000000000083403 (boot_l3pt_high) ✓
- boot_l3pt_high[0]=0x0000000000085403 (new_l2 at 0x85000) ✓
- new_l2[21]=0x0000000002b00803 (frame meta PA) ✓
- STILL FAULTS at level 2
```

### Problem Identified
The metadata VA range 0xffffe00000000000 is being routed through KPT[448]/KPT[511] → boot_l3pt_high → new_l2[21]. But boot_l3pt_high is for high VA range (0xffff800000000000+), not for metadata VA which should go through boot_l3pt_linear.

The frame metadata PA 0x2b00000 should be mapped via:
- KPT[511] → boot_l3pt_linear[0] → boot_l2pt_gb0[21] = 0x2b000403

But we're patching:
- KPT[448]/KPT[511] → boot_l3pt_high[0] → new_l2[21] = 0x2b00803

The routing is WRONG. Metadata VA should use boot_l3pt_linear, NOT boot_l3pt_high.

## Files Modified This Session
- `ostd/src/mm/page_table/mod.rs` — Added page table fixup code in `new_kernel_page_table()`
- `ostd/src/cpu/mod.rs` — Removed nested unsafe blocks (build fix)
- `ostd/src/arch/aarch64/trap/mod.rs` — EL1 handler with register dump

## Known Blockers
1. **Wrong page table path** — metadata VA going through boot_l3pt_high instead of boot_l3pt_linear
2. **PTE format confusion** — ARMv8 level 2/3 entry format (block vs table descriptor bits)
3. **DSB barriers needed** — Memory barrier after patching page tables

## Next Action
1. Route KPT[511] to boot_l3pt_linear (0x82000), NOT boot_l3pt_high (0x83000)
2. Set boot_l3pt_linear[0] to point to a new L2 table at 0x87000
3. Populate new L2 at 0x87000 with frame metadata entry at index 21
4. OR: Simply populate boot_l2pt_gb0[21] directly with frame metadata mapping

## Debug Markers Currently in Code
- `[npt] KPT[256]=`, `[npt] l3entry[0]=` — KPT slot 256 state
- `[npt] KPT[448]=`, `[npt] KPT[511]=` — high VA mappings
- `[npt] FIX: KPT[448] to`, `[npt] FIX: KPT[511] also` — fixup messages
- `[npt] boot_l3pt_high[0]=` — high L3 table entry
- `[npt] FIX: new_l2[...]` — new L2 entry fixups
- `[slot0] boot_root_pa=`, `[slot0] FIX: updating l3table[0]` — slot 0 fixups
- `[cursor] level=`, `[cursor] idx=`, `[cursor] pt_paddr=`, `[cursor] pt_vptr=`, `[cursor] pte_is_present=`, `[cursor] pte_paddr=`, `[cursor] next_pt_addr=` — cursor walk state
- `[kspace.m0]`, `[kspace.m1]`, `[kspace.m2]`, `[kspace.m3]` — kspace metadata init

## Recent Serial Output (Key Parts)
```
[npt] KPT[448]=0x0000000000000000
[npt] FIX: KPT[448] to 0x0000000000083403
[npt] KPT[511]=0x0000000000000000
[npt] FIX: KPT[511] also
[npt] boot_l3pt_high[0]=0x0000000000086003
[npt] FIX: boot_l3pt_high[0] to 0x0000000000085403
[npt] FIX: new_l2[0x15]=0x0000000002b00803
[npt] new_l2[0x15]=0x0000000002b00803
[cursor] level=0x04 idx=0x1c0 pt_paddr=0x337b000 pt_vptr=0xffff80000337b000
[cursor] pte_is_present=0x1 pte_paddr=0x83000
[cursor] level=0x03 idx=0x0 pt_paddr=0x83000 pt_vptr=0xffff800000083000
[cursor] pte_is_present=0x1 pte_paddr=0x85000
[EL1-SYNC] ESR=0x96000035 FAR=0x2b7d144
```

(End of file)
