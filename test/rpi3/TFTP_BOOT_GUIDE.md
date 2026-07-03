# Asterinas RPi3 TFTP Boot Guide

Boot Asterinas on Raspberry Pi 3B/3B+ via U-Boot + TFTP for rapid kernel iteration.
No SD card re-writes needed — just update the kernel on the TFTP server and reboot.

---

## Files in this directory

| File | Description |
|------|-------------|
| `u-boot.bin` | U-Boot 2023.07.02 for **RPi3B+** (LAN7515 + SMSC95xx + TFTP) |
| `u-boot-rpi3b.bin` | U-Boot 2023.07.02 for **RPi3B** (SMSC95xx LAN9514 + TFTP) |
| `boot.scr` | Compiled U-Boot boot script (DHCP + TFTP → booti) |
| `boot.cmd` | Source for boot.scr (edit this, then recompile) |
| `config.txt` | RPi firmware config (PL011 UART + U-Boot) |
| `firmware/` | bootcode.bin, start.elf, fixup.dat |

---

## Quick Setup

### Step 1: Prepare SD card (one-time)

```bash
# Identify SD card device
lsblk

# Format SD card as FAT32 (replace /dev/sdX with your device)
sudo mkfs.fat -F32 /dev/sdX1
sudo mkdir -p /mnt/rpi3boot
sudo mount /dev/sdX1 /mnt/rpi3boot

# Copy firmware files
sudo cp test/rpi3/firmware/bootcode.bin /mnt/rpi3boot/
sudo cp test/rpi3/firmware/start.elf    /mnt/rpi3boot/
sudo cp test/rpi3/firmware/fixup.dat    /mnt/rpi3boot/

# Copy config
sudo cp test/rpi3/config.txt   /mnt/rpi3boot/
sudo cp test/rpi3/cmdline.txt  /mnt/rpi3boot/   # if it exists

# Copy U-Boot (use u-boot-rpi3b.bin for RPi3B, u-boot.bin for RPi3B+)
sudo cp test/rpi3/u-boot.bin   /mnt/rpi3boot/

# Copy boot script
sudo cp test/rpi3/boot.scr     /mnt/rpi3boot/

sudo umount /mnt/rpi3boot
```

**Board selection:**
- **RPi3B+** (Gigabit Ethernet): use `u-boot.bin` (already the default)
- **RPi3B** (100Mbit Ethernet): replace with `sudo cp test/rpi3/u-boot-rpi3b.bin /mnt/rpi3boot/u-boot.bin`

---

### Step 2: Set up TFTP server on dev machine

```bash
# Install TFTP server
sudo apt-get install tftpd-hpa

# Default TFTP root is /srv/tftp/
sudo ls /srv/tftp/

# Copy Asterinas kernel to TFTP directory (repeat after each build)
sudo cp target/osdk/aster-nix/kernel8.img /srv/tftp/

# Verify TFTP server is running
sudo systemctl status tftpd-hpa

# Allow TFTP through firewall (if needed)
# sudo ufw allow 69/udp
```

---

### Step 3: Connect RPi3 to network

Connect RPi3's Ethernet port to the same network as the dev machine.

**Important:** U-Boot uses USB Ethernet (SMSC LAN9514 or LAN7515).
The Ethernet port on the RPi3 is connected via USB internally.
U-Boot's `usb start` command enables it.

---

### Step 4: Configure DHCP to point to TFTP server

U-Boot uses DHCP to get an IP and the TFTP server address. You need the DHCP server to return the dev machine's IP as the "next server" (option 66).

**Option A: Edit boot.cmd to hardcode server IP (simplest)**

```bash
# Edit the boot script source
vim test/rpi3/boot.cmd

# Uncomment and set this line:
# setenv serverip 192.168.1.100   ← set to your dev machine IP

# Recompile boot.scr
docker run --rm -v $(pwd)/test/rpi3:/rpi3 asterinas/aarch64-dev:latest bash -c "
  apt-get install -y -qq u-boot-tools 2>/dev/null &&
  mkimage -A arm64 -O linux -T script -C none -a 0 -e 0 \
    -n 'Asterinas RPi3 boot' -d /rpi3/boot.cmd /rpi3/boot.scr
"

# Copy new boot.scr to SD card
sudo mount /dev/sdX1 /mnt/rpi3boot
sudo cp test/rpi3/boot.scr /mnt/rpi3boot/
sudo umount /mnt/rpi3boot
```

**Option B: Configure router/dnsmasq DHCP option 66**

Add to `/etc/dnsmasq.conf`:
```
dhcp-option=66,<dev_machine_ip>   # TFTP next-server
```

---

### Step 5: Build and deploy kernel

```bash
# Build Asterinas kernel
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && \
  cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3"

# The kernel8.img is created by the aarch64-rpi3 scheme
# (raw binary via aarch64-linux-gnu-objcopy, linker at 0x80000)
# If that's not available yet, use manual conversion:
docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && \
  aarch64-linux-gnu-objcopy -O binary \
    target/aarch64-unknown-none-softfloat/release/aster-nix-osdk-bin \
    test/rpi3/kernel8.img"

# Deploy to TFTP
sudo cp test/rpi3/kernel8.img /srv/tftp/
```

---

## Boot flow

```
Power on RPi3
  → GPU loads bootcode.bin
  → GPU loads start.elf + config.txt
  → config.txt: kernel=u-boot.bin → GPU loads U-Boot
  → U-Boot 2023.07.02 starts
  → U-Boot auto-runs boot.scr from SD card
  → boot.scr: usb start → dhcp → tftpboot kernel8.img → booti
  → Asterinas kernel starts at 0x80000
  → P (PL011 init), V/T/Z/B diagnostics → aarch64_boot()
  → Kernel banner
```

---

## Expected U-Boot output

```
U-Boot 2023.07.02 (Sep 16 2023 - 11:12:25 +0900)

DRAM:  948 MiB
RPI 3 Model B+ (0xa020d3)
MMC:   mmc@7e202000: 0, mmc@7e300000: 1
Loading Environment from FAT... OK
In:    serial
Out:   serial
Err:   serial
Net:   No ethernet found.
starting USB...
Bus usb@7e980000: USB DWC2
scanning bus usb@7e980000 for devices...
      USB Device 0x424:7500 Found
Hit any key to stop autoboot:  0
Asterinas RPi3 U-Boot boot script starting...
Starting USB...
Running DHCP...
DHCP client bound to address 192.168.1.X (XXms)
IP: 192.168.1.X  Server: 192.168.1.Y
Downloading kernel8.img from TFTP server...
Using usb_ether device
TFTP from server 192.168.1.Y; ...
Bytes transferred = XXXX
Kernel loaded at 0x00080000, size XXXX bytes
Booting Asterinas...
## Loading kernel from FIT Image ...
P V T Z B
[a2-boot] entry
...
```

---

## Updating boot.cmd (recompile instructions)

After editing `boot.cmd`:

```bash
docker run --rm -v $(pwd)/test/rpi3:/rpi3 asterinas/aarch64-dev:latest bash -c "
  apt-get install -y -qq u-boot-tools 2>/dev/null &&
  mkimage -A arm64 -O linux -T script -C none -a 0 -e 0 \
    -n 'Asterinas RPi3 boot' -d /rpi3/boot.cmd /rpi3/boot.scr
"
```

---

## U-Boot binary sources

The pre-built binaries were downloaded from:
- https://github.com/takumin/rpi-uboot/releases/tag/v20230916

Both binaries include:
- USB DWC2 host controller
- SMSC95xx driver (RPi3B LAN9514)
- LAN78xx driver (RPi3B+ LAN7515)
- TFTP client
- DHCP client
- `booti` command (AArch64 Linux Image boot)

---

## Serial connection

- **Port**: GPIO 14 (TX), GPIO 15 (RX), GND
- **Baud rate**: 115200 (U-Boot + Asterinas)
- **Adapter**: 3.3V USB-TTL (NOT 5V!)
- **Connector**: GPIO pins 8 (TX→RX), 10 (RX→TX), 6 (GND)
