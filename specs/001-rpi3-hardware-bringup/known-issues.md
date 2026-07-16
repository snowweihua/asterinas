# RPi3 Known Issues

## Issue 1: Enable PL011 UART
**Description**: Currently using mini UART instead of PL011 UART for serial console.
**Impact**: PL011 has better features and is the proper UART for RPi3.
**Status**: Pending

## Issue 2: SimpleOnce Should Be Reverted to Spin::Once
**Description**: The `SimpleOnce` replacement for `spin::Once` was added as a workaround for RPi3 hang, but this should be reverted to use standard `spin::Once`.
**Impact**: Using custom SimpleOnce instead of well-tested spin::Once may introduce subtle bugs.
**Status**: Pending - needs investigation and fix

---
*Last updated: 2026-07-16*
