# RPi3 Known Issues

## Issue 1: Enable PL011 UART
**Description**: The bring-up currently uses the mini-UART for the serial console. PL011 is the more capable UART on RPi3 and is the long-term target.
**Impact**: None for v1.1 — mini-UART is functional and satisfies the current shell/serial requirements.
**Status**: Resolved (2026-08-26) — PL011 UART at GPIO 14/15 ALT0 is now the active serial console on RPi3. GPIO alt0 configuration, AUX peripheral management, and IRQ routing are implemented. Shell prompt and interactive commands work on PL011.

## Issue 2: SimpleOnce vs `spin::Once` on RPi3
**Description**: `ostd::sync::Once` dispatches to `boot::SimpleOnce` on RPi3 to avoid Cortex-A53 exclusive-atomic (`LDXR`/`STXR`) issues during single-core bring-up.
**Impact**: Confirmed correct for the current single-core baseline. It is an intentional RPi3-specific path, not a temporary workaround.
**Status**: Resolved — keep the RPi3 `SimpleOnce` path until SMP is enabled and exclusive-atomic support is validated on real hardware.

## Issue 3: Dynamic `busybox` `reboot -f` reliability
**Description**: Calling the dynamic `busybox` `reboot -f` applet sometimes faulted before reaching the `reboot(2)` syscall, producing intermittent "Segmentation fault" or no reset.
**Impact**: RPi3 reset was unreliable from the shell prompt.
**Status**: Resolved — a static `/bin/reboot` helper is now built into the initramfs; `reboot -f` from the shell reliably triggers PSCI `SYSTEM_RESET`.

---
*Last updated: 2026-08-17*
