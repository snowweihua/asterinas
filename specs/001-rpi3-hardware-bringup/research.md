# Research: RPi3 Hardware Bringup

## 1. D-Cache Coherency Gap on First Boot

### Decision
The first-boot PSCI reset reliability issue on RPi3 is caused by a D-cache coherency gap when U-Boot (running in cached mode) loads the kernel and initramfs into memory, then transfers control to the kernel with caches enabled.

### Rationale
On RPi3, U-Boot runs with caches enabled (I-cache and D-cache both on). When it loads `asterina.img` and `initramfs.cpio.gz` via TFTP into DRAM, those writes are buffered in the D-cache. When `booti` jumps to the kernel at PA `0x80000`, the kernel enables the MMU with its own page tables. The question is whether the D-cache data for the kernel code/text region is clean enough for the I-cache to fetch correctly.

On RPi3 BCM2837 (Cortex-A53), there is no I-cache vs D-cache coherency hardware (like CCI-400 on larger systems). If the kernel text is in D-cache but not yet cleaned to DRAM, the I-cache might see stale data (or vice versa).

The existing `boot.S` does NOT perform explicit cache maintenance before entering the kernel proper. The boot.S switches EL3→EL2→EL1, sets up page tables, enables MMU, then jumps to Rust code. There is no `ic IALLU` or `dc civac` before the first instruction fetch of the kernel proper.

The specific failure mode is likely:
1. U-Boot writes kernel/initramfs data via cached stores
2. `booti` jumps to kernel entry — kernel enables MMU
3. Kernel reads initramfs data from DRAM — but D-cache may not have been cleaned
4. CPIO extraction sees garbage or zeroes → init fails → kernel panic

### Alternatives Considered
- **Full cache disable**: Would be very slow; rejected
- **`dc ivac` (invalidate + clean)**: Not available on ARMv8.0 Cortex-A53 (only ARMv8.5+); rejected
- **`dc cvac` (clean + invalidate by VA)**: Clean cache lines for a VA range, then invalidate I-cache for same range — sufficient if the issue is D-cache lines not being written back to DRAM

### Fix Approach
Add explicit D-cache clean before enabling the MMU in `boot.S`, specifically for the memory region where initramfs is loaded. Also consider `ic iallu` (invalidate I-cache all) immediately after enabling MMU to ensure no stale instruction fetches.

---

## 2. BCM2836 Spin-Table SMP Bringup Failure

### Decision
The SMP secondary core bringup fails because the AP boot stub (at PA `0x40000`) is not correctly jumped to. The two-phase spin-table protocol in `smp_rpi3.rs` correctly writes the AP info region and triggers the mailbox IRQ, but the AP either never wakes or fails before reaching `ap_early_entry`.

### Rationale
The RPi3 uses the BCM2836 (the ARM component of BCM2837 SoC), not a standard PSCI implementation. The cpu-release-addr values from the DTB point to BCM2836 spin-table addresses (offsets `0x0D8` to `0x0F0` within `ARM_LOCAL` peripheral at `0x40000000`).

The protocol:
1. BSP copies AP boot stub to PA `0x40000` and cleans caches
2. BSP writes hold_flag=1, entry=`0x40000`, pt_root, info_array to AP info region at PA `0x50000`
3. BSP writes spin-table entry (AP boot stub PA) to the BCM2836 spin-table offset for each CPU
4. BSP triggers BCM2836 mailbox IRQ (`CORE0_MAILBOX_IRQCTL` at offset `0x84` for CPU 1)
5. BSP issues `sev` (send event) to wake APs from WFE

Issues identified:
- The AP boot stub (`ap_boot.S`) requires identity mapping in the low 4GB (`0x00000000` to `0xFFFFFFFF`). On RPi3 with LPAE/page tables, identity mapping of the low 4GB should exist in the boot page tables (see `boot.S` for the `PTE_DEVICE_2M` and `PTE_NORMAL_2M` entries for `0x00000000`-`0x7FFFFFFF`).
- The `sev` instruction on ARMv8 is sufficient to wake other CPUs from WFE, but only if they are in WFE and the event flag was set before they entered WFE.
- The AP boot stub copies itself to PA `0x40000` (physical), and after enabling MMU, it identity-maps the low 4GB. This should work if the boot page tables map `0x40000` correctly.
- The mailbox IRQ trigger writes to `ARM_LOCAL` + offset and issues `sev`. But on BCM2836, the standard ARM WFE/SEV mechanism may not work as expected — the BCM2836 has a custom interrupt router.

### Alternatives Considered
- **PSCI CPU_ON**: Not supported by the RPi3 firmware — the HVC #0 call returns but the AP never starts. The current code uses PSCI CPU_ON in `smp.rs` but this doesn't work on RPi3 hardware. The `smp_rpi3.rs` uses spin-table instead.
- **QEMU virt path**: Uses PSCI, which works in QEMU. But RPi3 hardware requires BCM2836 spin-table.

### Fix Approach
Trace the two-phase protocol end-to-end:
1. Add early debug putchars in `ap_boot.S` before and after MMU enable
2. Verify the AP actually reads from PA `0x50000` after waking
3. Verify the identity mapping covers both `0x40000` (boot stub) and `0x50000` (info region)
4. Consider replacing mailbox IRQ trigger with direct spin-table polling from BSP side

---

## 3. stat/lstat SIGSEGV on `busybox ls /bin`

### Decision
The SIGSEGV on `busybox ls /bin` is caused by a mismatch between the `struct stat` layout used by Asterinas and what busybox's C library (`musl`) expects on AArch64.

### Rationale
Asterinas AArch64 `struct Stat` (defined in `kernel/src/syscall/stat.rs`):
```rust
// AArch64 layout
st_dev (u64, 8B)  @0
st_ino (u64, 8B)  @8
st_mode (u32, 4B)  @16
st_nlink (u32, 4B) @20
st_uid (u32, 4B)  @24
st_gid (u32, 4B)   @28
st_rdev (u64, 8B)  @36  ← 4 bytes padding after st_gid for 8-alignment
__pad0 (u64, 8B)   @44
st_size (isize, 8B) @52
st_blksize (i32, 4B) @60
__pad1 (i32, 4B)   @64
st_blocks (isize, 8B) @68
st_atime (timespec, 16B) @76
st_mtime (timespec, 16B) @92
st_ctime (timespec, 16B) @108
Total: 124 bytes
```

Linux AArch64 `struct stat` (from `arch/arm64/include/uapi/asm/stat.h`):
```c
// Linux kernel layout
st_dev (8B)   @0
st_ino (8B)   @8
st_mode (4B)   @16
st_nlink (4B)  @20
st_uid (4B)    @24
st_gid (4B)    @28
__pad0 (4B)    @32  ← padding AFTER st_gid, BEFORE st_rdev
st_rdev (8B)   @36
st_size (8B)   @44
st_blksize (4B) @52
st_blksize (4B) @56  ← __pad1 (padding)
st_blocks (8B)  @60
st_atime (16B)  @68
st_mtime (16B)  @84
st_ctime (16B)  @100
Total: 116 bytes
```

The key mismatch: In Asterinas, `__pad0` is placed AFTER `st_rdev` (at offset 44). In Linux, `__pad0` is placed BEFORE `st_rdev` (at offset 32). This shifts all subsequent fields by 12 bytes in Asterinas vs. Linux.

Additionally, `st_size` is at offset 52 in Asterinas but offset 44 in Linux — an 8-byte discrepancy. When busybox writes `st_size` via the stat syscall, it writes to Linux's expected offset (44), but Asterinas reads from offset 52. The `st_size` read by busybox is actually `st_blksize` in Asterinas, which is likely garbage or zero.

### Resolution
The AArch64 `Stat` layout in `kernel/src/syscall/stat.rs` was fixed and now:
- Includes the glibc reserved tail to reach the full 128-byte struct size.
- Places `__pad0` before `st_rdev` (offset 32) and `__pad1` before `st_size` as required.
- Adds compile-time assertions for the Linux AArch64 offsets.
`ls /bin` and `ls -la /` now return correctly on RPi3 hardware.

---

## 4. reboot Syscall (169) Not Implemented

### Decision
The reboot syscall (syscall 142 on AArch64 Linux) is completely missing from the syscall dispatch table. The syscall returns `ENOSYS`.

### Rationale
On AArch64 Linux, the `reboot` syscall number is 142. Looking at `kernel/src/syscall/arch/aarch64.rs`, syscall 142 is mapped to `sys_getsid` — not reboot. There is no `reboot` module in the syscall list.

On AArch64, `reboot` typically invokes PSCI `SYSTEM_RESET` function (function ID `0x84000009`). This is the correct approach for RPi3.

The PSCI infrastructure already exists in `smp.rs` (`psci_call` function using `hvc #0`). The `PSCI_SYSTEM_OFF` function ID is `0x84000008` and `PSCI_SYSTEM_RESET` is `0x84000009`.

### Resolution
Implemented in `kernel/src/syscall/reboot.rs` and registered in `kernel/src/syscall/mod.rs` and `arch/aarch64.rs` as syscall 142:
- `RB_AUTOBOOT` (0x1234567) and `RB_RESTART` (0x01234567) → `psci_system_reset`.
- `RB_POWER_OFF` (0x43211234) → `psci_system_off`.
- On RPi3 the PSCI conduit is `smc #0` (not HVC) with function ID `0x8400_0009` for reset and `0x8400_0008` for power-off.
- A static `/bin/reboot` helper is built into the AArch64 initramfs so `reboot -f` from the shell reliably reaches the syscall without dynamic busybox/glibc faults.

---

## Summary of Required Fixes

| # | Issue | File(s) to Modify | Approach |
|---|-------|------------------|----------|
| 1 | First-boot D-cache coherency | `ostd/src/arch/aarch64/boot/boot.S` | Add `dc cvac` for initramfs region + `ic iallu` after MMU enable |
| 2 | SMP AP never starts | `ostd/src/arch/aarch64/boot/smp_rpi3.rs`, `ap_boot.S` | Debug trace; verify spin-table reads; check BCM2836 mailbox IRQ |
| 3 | stat struct layout | `kernel/src/syscall/stat.rs` | Fixed — correct AArch64 Stat layout with compile-time offset assertions |
| 4 | reboot syscall missing | `kernel/src/syscall/reboot.rs` + mod.rs + aarch64.rs | Implemented — PSCI SYSTEM_RESET via `smc #0`; initramfs uses a static `/bin/reboot` helper |
