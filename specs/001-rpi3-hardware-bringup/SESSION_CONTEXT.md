# RPi3 Debug Session - 2026-07-24 (Session 7) — Build MCP Path Fixed, TTBR1 LMA Investigation

## Current Task T030
Boot to shell with interactive command working.

## Session 7 Summary

### What Works (Fixed This Session)
1. **Build MCP path fixed** (`tools/build_mcp_server.py`): `DEFAULT_RAW_PATH` changed from `/home/snow/asterinas/target/osdk/aster-nix/asterina.img` to `/tmp/asterina.img`. The target directory is owned by root and not writable, causing objcopy to fail with "Bad file descriptor". Now `build_and_deploy` works end-to-end (build → convert → deploy all succeed).

2. **LMA=0x80000 confirmed in ELF**: OSDK's `aarch64-rpi3.ld.template` linker script correctly sets `KERNEL_LMA=0x80000`. The `.boot` section ELF LOAD offset is 0x80000.

3. **OSDK base crate linker override**: Docker build copies `aarch64-rpi3.ld.template` to the base crate's `aarch64.ld` before building, ensuring correct LMA.

### What's Still Blocked (Persistent Crash at init_array)
- **Crash INSIDE `aster_block` init function body** (not in the call machinery)
- ELR=0x3AF610A8 — same address as previous sessions
- TTBR1 boot page table maps VA `0xffff000000XXXXXX` → PA `0x40000000+XXXXXX` (hardcoded for QEMU virt LMA=0x40080000)
- With actual LMA=0x80000, kernel code is at PA 0xceacc (within low memory), but TTBR1 tries to access PA `0x40000000+0x0ceacc = 0x400ceacc` (wrong!)
- **Root cause hypothesis**: Boot page table TTBR1 mapping is WRONG for RPi3's LMA=0x80000. The `boot_l2pt_gb0` entries map PA 0x0-0x3FFFFFFF as 2MB blocks (correct for actual RAM), but TTBR1 entry 0 maps VA 0xffff000000000000→0xffff00003FFFFFFF to PA 0x40000000+XXXXXX (wrong offset for LMA=0x80000).

### TTBR1 Boot Page Table Analysis
- `boot_l4pt` slot 256 → `boot_l3pt_high` for VA 0xffff000000000000+
- `boot_l3pt_high` entry 0 → `boot_l2pt_gb0` (correct: maps via L2 table)
- `boot_l2pt_gb0` correctly maps PA 0x00000000-0x3FFFFFFF as 2MB blocks (covering actual kernel at PA 0xceacc)
- **BUT**: The question is whether the init function pointers being called are correct VA→PA translations

### Next Steps to Try
1. **Read init function code before calling** — add debug to read first few instructions at fn_val address BEFORE calling, to verify the VA translation is correct
2. **Update TTBR1 mapping in boot.S** — adjust `boot_l3pt_high` entry 0 to map VA range to correct PA range for LMA=0x80000 (though analysis suggests boot_l2pt_gb0 should already be correct)
3. **Bypass `invoke_ffi_init_funcs()`** — skip init_array entirely to see if boot proceeds further

### Tools & Infrastructure
- `tools/build_mcp_server.py` — Unified build+deploy MCP server on port 8912
- `tools/serial_mcp_server.py` — MCP HTTP server on port 8910 for reading RPi3 serial console
- `.reasonix/config.toml` — MCP plugin registrations

### Boot Log (Current State)
```
[init.C] after kspace::init
[sync.init.0] start
[sync.init.1] after rcu::init
[init] after sync::init
[dma.init.0] start
[dma.init.1] done (skipped on aarch64)
[init.dma] after dma::init
[late.0] start
[late.trap] after trap::init
[late.1] after construct_io_mem_allocator_builder
[late.2] after boot_all_aps
[late.3] after timer::init
[late.4] after io::init
[IFF.len=N] — init_array iteration count
[IFF.0 ptr=0xXXXXXXXX] — function pointer being called
[IFF.CALL]
[IFF.read=0xXXXXXXXX] — first word read from function address
[CRASH] — inside aster_block init function
```

### Files Modified This Session
- `tools/build_mcp_server.py` — Fixed DEFAULT_RAW_PATH from target/.../asterina.img to /tmp/asterina.img (writable location)
- `ostd/src/lib.rs` — Added debug in invoke_ffi_init_funcs: prints init_array contents, reads first word before calling, wraps pl011_puts in unsafe blocks

### Git Log
```
d04f2504 aarch64: fix LMA to 0x80000 (OSDK fix + base_crate linker script fix)
71d681ae T030 session end: save session state, update workflow for build MCP
41c7a709 T030: fix boot crash on RPi3 — generalized LDARB/LDXR workaround
405031d5 aarch64/npt: bypass PT spinlock on RPi3 — Cortex-A53 exclusive-monitor issue
2e9dc318 aarch64/once: bypass SimpleOnce::load(Acquire) — LDARB faults on RPi3
c1b4299e aarch64/boot: replace 1GiB block entry in boot_l3pt_high with L2 table
77a38d99 aarch64/meta: fix frame_paddr() during bootstrap on RPi3 (frame_paddr_base==0)
```

### Next Session Action
1. Rebase work on session context — the TTBR1 analysis shows boot_l2pt_gb0 correctly maps PA 0x0-0x3FFFFFFF but the question is whether init function VAs are correctly resolved
2. Add debug to print the actual instruction bytes at the init function address to verify VA→PA translation is working
3. Consider bypassing invoke_ffi_init_funcs entirely to see if boot reaches shell without init_array
