#!/bin/bash
# embed_initramfs_simple.sh - Embed initramfs into kernel8.img using rust-objcopy
#
# Usage: ./embed_initramfs_simple.sh [initramfs.cpio.gz] [kernel_elf] [output_img]
#   Embeds initramfs into the .initramfs section, then converts to raw binary.

set -e

INITRAMFS="${1:-../../test/build/initramfs.cpio.gz}"
KERNEL_ELF="${2:-../../target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf}"
OUTPUT_IMG="${3:-../../target/osdk/aster-nix/kernel8.img}"

if [ ! -f "$INITRAMFS" ]; then
    echo "Error: Initramfs not found: $INITRAMFS"
    echo "Build with: make build OSDK_TARGET_ARCH=aarch64"
    exit 1
fi

if [ ! -f "$KERNEL_ELF" ]; then
    echo "Error: Kernel ELF not found: $KERNEL_ELF"
    echo "Build with: cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3"
    exit 1
fi

# Find rust-objcopy
OBJCOPY=$(find /root/.rustup -name rust-objcopy 2>/dev/null | head -1)
if [ -z "$OBJCOPY" ]; then
    echo "Error: rust-objcopy not found in /root/.rustup"
    exit 1
fi

echo "=== Embedding initramfs into kernel ==="
echo "Initramfs: $INITRAMFS ($(stat -c%s "$INITRAMFS") bytes)"
echo "Kernel:    $KERNEL_ELF"
echo "Output:    $OUTPUT_IMG"
echo ""

# Create a copy to modify
TMP_ELF="/tmp/kernel_with_initramfs.elf"
cp "$KERNEL_ELF" "$TMP_ELF"

# Embed initramfs into .initramfs section
$OBJCOPY \
    --update-section .initramfs="$INITRAMFS" \
    --set-section-flags .initramfs=alloc,load,readonly,data \
    "$TMP_ELF"

# Convert to raw binary
$OBJCOPY -O binary "$TMP_ELF" "$OUTPUT_IMG"

# Cleanup
rm -f "$TMP_ELF"

echo "Done! Output: $OUTPUT_IMG ($(stat -c%s "$OUTPUT_IMG") bytes)"
