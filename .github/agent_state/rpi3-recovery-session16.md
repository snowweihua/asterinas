# RPi3 Debug Recovery — 2026-07-31 Session 16

## WHERE WE ARE
Fixing kernel boot crash in `kspace::init_kernel_page_table()`. The cursor crashes when trying to map frame metadata VA range.

## THE BUG
Frame metadata VA 0xffffe00000000000 should go through:
  KPT[511] → boot_l3pt_linear[0] → boot_l2pt_gb0[21] → frame metadata PA 0x2b00000

But current code routes it through:
  KPT[511] → boot_l3pt_high[0] → new_l2[21] → (still faults)

## KEY FILES
- `ostd/src/mm/page_table/mod.rs` — Page table fixup code (lines ~430-530)
- `ostd/src/mm/kspace/mod.rs` — kspace init code
- `ostd/src/arch/aarch64/trap/mod.rs` — EL1 exception handler

## ROOT CAUSE
Metadata VA uses the wrong page table hierarchy. boot_l3pt_high is for kernel high VA range (0xffff800000000000+), not for metadata range.

## THE FIX NEEDED
1. KPT[511] should point to boot_l3pt_linear (PA 0x82000), not boot_l3pt_high (0x83000)
2. boot_l3pt_linear[0] should point to a new L2 at 0x87000
3. new L2 at 0x87000[21] = 0x2b000403 (frame metadata 2M block)

## CURRENT STATE
- KPT[256]=0x85003 ✓
- boot_l3pt_linear[0]=0x84003 (FIXED) ✓
- boot_l2pt_gb0[25]=0x337b0403 (FIXED) ✓
- KPT[448]=0x83003 (WRONG - should be 0x82003)
- KPT[511]=0x83003 (WRONG - should be 0x82003)
- boot_l3pt_high[0]=0x85003 (WRONG routing)
- new_l2[21]=0x2b00803 (PTE format may be wrong)

## COMMIT IF NEEDED
aarch64/kspace: route metadata VA through boot_l3pt_linear not boot_l3pt_high

(End of file)
