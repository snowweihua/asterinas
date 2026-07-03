#!/bin/bash
# test_qemu_raspi3b.sh - QEMU raspi3b boot validation for RPi3 kernel8.img
#
# Usage: ./test_qemu_raspi3b.sh [kernel8.img]
#   Default kernel: target/osdk/aster-nix/kernel8.img
#
# NOTE: QEMU raspi3b has limited emulation. It does NOT emulate:
#   - BCM2836 Local Interrupt Controller
#   - mini-UART (GPIO 14/15)
#   - Full peripheral set
# This test is ONLY for verifying boot.S, page tables, and early boot code.
# Real hardware testing is required for timer, interrupts, and serial output.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KERNEL_IMG="${1:-${SCRIPT_DIR}/../../target/osdk/aster-nix/kernel8.img}"
TIMEOUT_SECS=30

if [ ! -f "$KERNEL_IMG" ]; then
    echo "Error: Kernel image not found: $KERNEL_IMG"
    echo "Build with: cargo osdk build --release --target-arch aarch64 --boot-method RawBinary --scheme aarch64-rpi3"
    exit 1
fi

echo "=== QEMU raspi3b Boot Validation ==="
echo "Kernel: $KERNEL_IMG"
echo "Timeout: ${TIMEOUT_SECS}s"
echo ""

# Create temporary log file
LOG_FILE=$(mktemp)
trap "rm -f $LOG_FILE" EXIT

# Run QEMU raspi3b with timeout
# -machine raspi3b: Raspberry Pi 3B machine type
# -kernel: load kernel8.img at PA 0x80000
# -nographic: no GUI
# -serial stdio: redirect serial to stdout (captured)
echo "[1/3] Starting QEMU raspi3b..."
if timeout "$TIMEOUT_SECS" qemu-system-aarch64 \
    -machine raspi3b \
    -kernel "$KERNEL_IMG" \
    -nographic \
    -serial file:"$LOG_FILE" \
    2>&1; then
    QEMU_EXIT=$?
else
    QEMU_EXIT=$?
fi

# timeout returns 124 if the command timed out
if [ "$QEMU_EXIT" -eq 124 ]; then
    echo "[2/3] QEMU timed out after ${TIMEOUT_SECS}s (expected for kernel without exit)"
else
    echo "[2/3] QEMU exited with code $QEMU_EXIT"
fi

# Check log for any output
echo "[3/3] Checking serial output..."
LOG_SIZE=$(wc -c < "$LOG_FILE" | tr -d ' ')

if [ "$LOG_SIZE" -gt 0 ]; then
    echo "PASS: Serial output detected ($LOG_SIZE bytes)"
    echo ""
    echo "--- First 200 bytes of output ---"
    head -c 200 "$LOG_FILE" | xxd | head -20
    echo "--- End of output preview ---"
    exit 0
else
    echo "FAIL: No serial output detected"
    echo ""
    echo "Possible causes:"
    echo "  - boot.S crashed before UART init"
    echo "  - Page table setup incorrect for RPi3 memory map"
    echo "  - Kernel linked to wrong address"
    echo ""
    echo "Debugging tips:"
    echo "  1. Verify kernel8.img was built with aarch64-rpi3 scheme"
    echo "  2. Check linker script has KERNEL_LMA = 0x80000"
    echo "  3. Ensure early_puts uses mini-UART PA 0x3F215000"
    exit 1
fi
