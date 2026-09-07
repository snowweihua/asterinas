# Recovery note — 2026-09-07, RPi3 `ls` verified working

## Proven by fresh boot (kernel 3bc2f0 + auto-ls initramfs)
- Shebang argv fix VERIFIED: `/init` script executed (`sys_read(fd=10) -> 186` = script bytes).
- `ls /` → `bin dev etc init nix proc root run sys tmp`, exit 0.
- `ls /bin` → busybox applets, exit 0. Stat-struct SIGSEGV hypothesis REFUTED.
- Shell then blocks in `ppoll` on stdin; serial RX bytes never arrive (timer 1000Hz FIFO poll sees nothing).

## Tree state
- KEEP: `kernel/src/process/program_loader/mod.rs` shebang fix (push script path into new_argv).
- REVERTED (debug printlns): driver/mod.rs, fs/mod.rs, job_control.rs, posix_thread/exit.rs, syscall/{execve,exit,exit_group,ioctl,open,poll,preadv,read}.rs.
- TFTP: `/mnt/d/pi_sd/asterina.img` (debug build), `/mnt/d/pi_sd/initramfs.cpio` (auto-ls; backup `initramfs.cpio.bak-before-autols`).
- Board: powered ON, shell idle at `/ #` prompt.

## Next
1. Rebuild clean kernel (shebang fix only) → convert → deploy → fresh boot → confirm AUTO-TEST output without noise.
2. Commit shebang fix.
3. QEMU smoke test regression (`~ #` vs `/ #` prompt mismatch suspected).
