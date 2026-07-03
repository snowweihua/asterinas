#!/bin/bash
# embed_initramfs.sh - Embed an initramfs CPIO archive into kernel ELF
#
# Usage: ./embed_initramfs.sh <kernel_elf> <initramfs.cpio.gz> <output_elf>
#
# Example:
#   ./embed_initramfs.sh \
#     target/osdk/aster-nix/aster-nix-osdk-bin.elf \
#     test/build/initramfs.cpio.gz \
#     target/osdk/aster-nix/aster-nix-osdk-bin-with-initramfs.elf

set -euo pipefail

if [ $# -lt 3 ]; then
    echo "Usage: $0 <kernel_elf> <initramfs.cpio.gz> <output_elf>"
    exit 1
fi

KERNEL_ELF="$1"
INITRAMFS="$2"
OUTPUT_ELF="$3"

if [ ! -f "$KERNEL_ELF" ]; then
    echo "Error: Kernel ELF not found: $KERNEL_ELF"
    exit 1
fi

if [ ! -f "$INITRAMFS" ]; then
    echo "Error: Initramfs not found: $INITRAMFS"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

echo "=== Embedding initramfs into kernel ELF ==="
echo "Kernel:   $KERNEL_ELF"
echo "Initramfs: $INITRAMFS"
echo "Output:   $OUTPUT_ELF"

# Step 1: Convert initramfs to an object file with symbols
echo "[1/3] Converting initramfs to object file..."
aarch64-linux-gnu-objcopy \
    -I binary \
    -O elf64-littleaarch64 \
    -B aarch64 \
    --rename-section .data=.initramfs,alloc,load,readonly,data \
    --redefine-sym _binary__initramfs_start=__initramfs_start \
    --redefine-sym _binary__initramfs_end=__initramfs_end \
    "$INITRAMFS" "$TMP_DIR/initramfs.o"

# Step 2: Link the object file into the kernel ELF
echo "[2/3] Linking initramfs into kernel ELF..."
aarch64-linux-gnu-ld \
    -T "$SCRIPT_DIR/../../osdk/src/base_crate/aarch64-rpi3.ld.template" \
    -o "$OUTPUT_ELF" \
    "$KERNEL_ELF" "$TMP_DIR/initramfs.o"

# Step 3: Verify symbols exist
echo "[3/3] Verifying embedded initramfs symbols..."
if aarch64-linux-gnu-nm "$OUTPUT_ELF" | grep -q __initramfs_start; then
    INITRAMFS_SIZE=$(aarch64-linux-gnu-nm "$OUTPUT_ELF" | grep __initramfs_end | awk '{print $1}' | sed 's/^0*//' | head -1)
    echo "PASS: initramfs embedded successfully"
    echo "  __initramfs_start symbol found"
    echo "  __initramfs_end symbol found"
else
    echo "FAIL: initramfs symbols not found in output ELF"
    exit 1
fi

echo ""
echo "=== Done ==="
echo "Output: $OUTPUT_ELF"
