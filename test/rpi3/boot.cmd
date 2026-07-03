# Asterinas RPi3 U-Boot TFTP Boot Script
#
# This script:
#   1. Starts USB and waits for the USB Ethernet adapter to enumerate
#   2. Acquires an IP via DHCP
#   3. Downloads kernel8.img from the TFTP server
#   4. Downloads initramfs.cpio.gz from the TFTP server
#   5. Boots with booti (AArch64 Linux Image format)
#
# The DTB is provided by RPi firmware (passed to U-Boot in fdt_addr_r),
# so we don't need a separate tftpboot for the DTB.
#
# Usage:
#   - Set TFTP server IP below OR rely on DHCP option 66 (next-server)
#   - Put kernel8.img in TFTP server root directory
#   - Compile: mkimage -A arm64 -O linux -T script -C none -a 0 -e 0 \
#                -n "Asterinas RPi3 boot" -d boot.cmd boot.scr
#
# TFTP server setup (on dev machine):
#   sudo apt-get install tftpd-hpa
#   sudo cp kernel8.img /srv/tftp/
#   # DHCP server should set option 66 to dev machine IP, or edit serverip below

echo "Asterinas RPi3 U-Boot boot script starting..."

# Step 1: Start USB subsystem to detect USB Ethernet adapter
echo "Starting USB..."
usb start

# Step 2: Acquire IP via DHCP, then override serverip to point to TFTP server
echo "Running DHCP..."
dhcp
# Hardcode TFTP server IP (Windows machine at 10.142.15.21, root: D:/pi_sd/)
setenv serverip 10.142.15.21

if test $? -ne 0; then
    echo "DHCP failed! Check Ethernet connection."
    echo "Try: setenv serverip <IP>; setenv ipaddr <IP>; tftpboot ..."
    sleep 5
fi

echo "IP: ${ipaddr}  Server: ${serverip}"

# Step 3: Download kernel via TFTP
echo "Downloading kernel8.img from TFTP server ${serverip}..."
tftpboot ${kernel_addr_r} kernel8.img

if test $? -ne 0; then
    echo "TFTP failed! kernel8.img not found on server ${serverip}"
    echo "Check: sudo cp kernel8.img /srv/tftp/"
    sleep 10
fi

echo "Kernel loaded at ${kernel_addr_r}, size ${filesize} bytes"

# Step 4: Download initramfs via TFTP
# Using uncompressed CPIO to avoid miniz_oxide AArch64 inflate bug.
echo "Downloading initramfs.cpio from TFTP server ${serverip}..."
tftpboot ${ramdisk_addr_r} initramfs.cpio
setenv initramfs_size ${filesize}
echo "Initramfs loaded at ${ramdisk_addr_r}, size ${initramfs_size} bytes"

# Step 5: Load DTB from SD card
echo "Loading DTB from SD card..."
fatload mmc 0:1 ${fdt_addr_r} bcm2710-rpi-3-b.dtb

# Step 6: Boot the kernel with initramfs
# Set bootargs explicitly to avoid a leading-space leftover from the U-Boot environment.
setenv bootargs "init=/init"
echo "Booting Asterinas..."
booti ${kernel_addr_r} ${ramdisk_addr_r}:${initramfs_size} ${fdt_addr_r}

echo "booti failed! kernel may not be a valid AArch64 Linux Image."
