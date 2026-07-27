# RPi3 Debug Session - 2026-07-24 (Session 9)

## Current Task T030
Boot to shell on RPi3 — persistent crash at `elr=0x3af610a8` (Instruction Abort).

## Session 9 Summary

### What's Working
1. **Power MCP fixed**: USB power switch device was not responding — user fixed hardware/USB issue
2. **activate_kernel_page_table enabled**: Now reaches `[fa.0]-[fa.5]` consistently
3. **Root cause identified**: The **kernel code VA→PA mapping in slot 0 is wrong**

### Critical Finding: Slot 0 Boot PT Points to Wrong PA

Debug markers in `new_kernel_page_table()` reveal:
```
[slot0] boot_root_pa=0x0000000000082000
[slot0] pte=0x0000000000084003    (valid table descriptor → L3@0x84000)
[slot0] l3table_pa=0x0000000000084000
[slot0] l3table[0]=0x0000000000086003  (maps kernel VA 0xffff_0000_0000_0000 → PA 0x86000)
```

**But the kernel is actually at PA 0x80000** (loaded by U-Boot at 0x80000).

The slot 0 L3 table maps kernel code VA to **PA 0x86000**, but the kernel code is at **PA 0x80000**. This is a 0x6000 (24568) byte mismatch.

### Why This Matters

When `activate_page_table()` switches TTBR1 to the new KPT:
1. New KPT slot 0 → L3 table (copied from boot)
2. L3[0] → PA 0x86000
3. CPU tries to fetch instruction at elr=0xffff_0000_0309_850
4. MMU walks: VA → L3[0] → PA 0x86000 + offset → PA 0x86000 + 0x9850 = PA 0x3af610a8... wait, that's different

Actually the MMU would translate VA 0xffff_0000_0309_850:
- Bits[47:39]=0 → L4[0] → L3 table
- Bits[38:30]=0 → L3[0] → block/page
- VA 0xffff_0000_0309_850 offset in 4KB page = 0x9850
- If L3[0] is a 4KB page pointing to 0x86000, then PA = 0x86000 + 0x9850 = 0x8f850

But elr reported is 0x3af610a8. Hmm.

Actually wait — the ESR says Instruction Abort. The CPU couldn't fetch the instruction. This could mean:
1. The PTE for the VA is marked invalid/not present
2. The PTE is present but points to an invalid PA
3. Some other MMU fault

But L3[0]=0x86003 IS a valid present PTE. Unless... the TYPE bit (bit 1) isn't set properly, making the hardware treat it as invalid.

### Kernel Physical Base Calculation is Wrong

The `kernel_loaded_offset()` returns `0xffff_0000_0000_0000`. This means:
- `kernel_physical_base(__kernel_start=0xffff_0000_0309_850, offset=0xffff_0000_0000_0000)`
- = `dram_base + (__kernel_start - offset)` = `0x4000_0000 + (0xffff_0000_0309_850 - 0xffff_0000_0000_0000)`
- = `0x4000_0000 + 0x309_850` = `0x4309_850`

This says the kernel is at PA 0x4309_850, but it's actually at 0x80000.

The issue is that `kernel_loaded_offset` should equal `__kernel_start - actual_load_paddr`, which would be:
- `0xffff_0000_0309_850 - 0x80000 = 0xffff_0000_0000_0000`... wait, that's the same.

Actually: `0xffff_0000_0309_850 - 0x80000 = 0xffff_0000_0309_850 - 0x80000`

Hmm, let me think: `0xffff_0000_0309_850 - 0x80000`:
- In AArch64 with virtual address arithmetic: `0xffff_0000_0309_850 - 0x0000_0000_008_0000 = 0xffff_0000_0289_850`

So `kernel_loaded_offset()` should be `0xffff_0000_0289_850` not `0xffff_0000_0000_0000`.

But that still doesn't make sense because the DRAM base is 0x4000_0000 and the kernel is at 0x80000.

Actually, the LINEAR mapping on AArch64 maps VA 0xffff_8000_0000_0000 to PA 0x0. So PA 0x80000 maps to VA 0xffff_8000_0000_0000 + 0x80000 = 0xffff_8000_0008_0000.

But the kernel code is linked at VA 0xffff_0000_xxxx, NOT 0xffff_8000_xxxx. This is the fundamental mismatch.

The kernel code is at VA 0xffff_0000_0309_850. The kernel code VA range is 0xffff_0000_0000_0000 to 0xffff_0000_ffff_ffff. This range is in the "high VA" region (top half of VA space).

The boot page table's slot 0 L3 table maps this range. But the physical address it maps to (0x86000) doesn't match where the kernel actually is (0x80000).

### Files Modified
- `ostd/src/mm/kspace/mod.rs`: Enabled kernel code mapping section for AArch64 (removed `#[cfg(not(target_arch = "aarch64"))]`) — but cursor_mut crashes, so currently skipped with debug marker
- `ostd/src/mm/page_table/mod.rs`: Added slot0 PTE debug markers (boot_root_pa, pte, l3table_pa, l3table[0])
- `ostd/src/mm/page_table/node/mod.rs`: Debug markers [fa.0]-[fa.5] in first_activate

### Git Log (Recent)
```
55cc58b9 aarch64/rpi3: add slot0 PTE debug markers — reveals boot PT maps kernel to PA 0x86000 instead of actual 0x80000
7395395e docs: update AGENTS.md with power MCP workflow and commands
ca340cd3 T030 session 8: power MCP working, init bypass reveals crash in register_ap_entry
2b35362b tools: add power_mcp_server.py for USB power switch control
b3554b59 tools: fix build_mcp DEFAULT_RAW_PATH to /tmp — fixes objcopy Bad file descriptor
```

### Next Session Action
1. **Fix kernel_physical_base calculation**: The offset computation assumes kernel is at a high PA, but it's at PA 0x80000. Need to determine the correct `kernel_loaded_offset` that makes the kernel region point to PA 0x80000.
2. **OR: Don't rely on slot 0 copy**. Instead, properly add the kernel code mapping to the new KPT. The cursor_mut approach crashed — need to debug why OR use a different approach (direct PTE write).
3. **The cursor_mut crash**: When the kernel code mapping section was enabled, it crashed INSIDE `cursor_mut()` call (between [kspace.g] and [kspace.h]). This needs separate investigation.

### Boot Log (Current)
```
[kspace.W] start
[ek] empty_kernel start
[npt] after empty_kernel
[slot0] boot_root_pa=0x0000000000082000
[slot0] pte=0x0000000000084003
[slot0] l3table_pa=0x0000000000084000
[slot0] l3table[0]=0x0000000000086003
[kspace.X] after new_kernel_page_table
[kspace.Y] after disable_preempt
[kspace.d] after meta mapping
[kspace.e] skipped (AArch64 uses slot0 copy)
[init.C] after kspace::init
...
[fa.0] start first_activate
[fa.1] paddr=0x000000000337b000
[fa.2] calling activate_page_table
[fa.3] after ap
[fa.4] x30=0xffff000000309850
[fa.5] sp=0xffff0000000c7e40
[CRASH elr=0x3af610a8 ESR=0x02000000]
```
