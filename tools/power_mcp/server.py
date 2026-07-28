#!/usr/bin/env python3
"""MCP server for controlling RPi3B power via USB-controlled power switch.

The power switch uses a simple serial protocol on /dev/ttyACM0:
  - Power ON:  0xA0 0x01 0x01 0xA2  then  0xA0 0x02 0x01 0xA3
  - Power OFF: 0xA0 0x01 0x00 0xA1  then  0xA0 0x02 0x00 0xA2
  - Status:    0xA0 0x01 0x02 0xA3  then  0xA0 0x02 0x02 0xA4 (returns CH1:ON/OFF CH2:ON/OFF)

Run directly as a script (uses stdio protocol):
  python3 -m power_mcp.server
"""

import os
import time
import termios

from fastmcp import FastMCP

TTY_DEVICE = "/dev/ttyACM0"
ON_SEQ = [b"\xa0\x01\x01\xa2", b"\xa0\x02\x01\xa3"]
OFF_SEQ = [b"\xa0\x01\x00\xa1", b"\xa0\x02\x00\xa2"]
STATUS_SEQ = [b"\xa0\x01\x02\xa3", b"\xa0\x02\x02\xa4"]
LOG_FILE = "/home/snow/asterinas/tools/logs/power_mcp.log"


def log(msg):
    os.makedirs(os.path.dirname(LOG_FILE), exist_ok=True)
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")


log("=== Power MCP server starting ===")


def wait_for_device(path, timeout=5.0):
    start = time.time()
    while time.time() - start < timeout:
        if os.path.exists(path):
            try:
                fd = os.open(path, os.O_RDWR | os.O_NOCTTY)
                termios.tcflush(fd, termios.TCIOFLUSH)
                os.close(fd)
                return True
            except OSError:
                pass
        time.sleep(0.2)
    return False


def send_raw_bytes(data: bytes):
    if not wait_for_device(TTY_DEVICE):
        raise OSError(f"Device {TTY_DEVICE} not available")
    fd = os.open(TTY_DEVICE, os.O_RDWR | os.O_NOCTTY)
    try:
        attrs = termios.tcgetattr(fd)
        attrs[0] = 0
        attrs[1] = 0
        attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
        attrs[3] = 0
        attrs[4] = termios.B115200
        attrs[5] = termios.B115200
        attrs[6][termios.VMIN] = 0
        attrs[6][termios.VTIME] = 10
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
    except termios.error:
        pass
    os.write(fd, data)
    os.close(fd)
    time.sleep(1)


def power_on() -> str:
    for cmd in ON_SEQ:
        send_raw_bytes(cmd)
    return "OK: power ON"


def power_off() -> str:
    for cmd in OFF_SEQ:
        send_raw_bytes(cmd)
    return "OK: power OFF"


def power_status() -> str:
    if not os.path.exists(TTY_DEVICE):
        return "ABSENT"
    st = os.stat(TTY_DEVICE)
    result = f"PRESENT (mode={oct(st.st_mode)})"

    if not wait_for_device(TTY_DEVICE):
        return result + " (device not accessible)"

    fd = os.open(TTY_DEVICE, os.O_RDWR | os.O_NOCTTY)
    try:
        attrs = termios.tcgetattr(fd)
        attrs[0] = 0
        attrs[1] = 0
        attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
        attrs[3] = 0
        attrs[4] = termios.B115200
        attrs[5] = termios.B115200
        attrs[6][termios.VMIN] = 0
        attrs[6][termios.VTIME] = 5
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
    except termios.error:
        pass

    termios.tcflush(fd, termios.TCIOFLUSH)

    responses = []
    for cmd in STATUS_SEQ:
        os.write(fd, cmd)
        time.sleep(0.3)
        d = os.read(fd, 100)
        if d:
            responses.append(d.decode("utf-8", errors="replace").strip())

    os.close(fd)

    if responses:
        result += " | " + " ".join(responses)

    return result


mcp = FastMCP(
    name="Power MCP",
    instructions="MCP server for controlling RPi3B power via USB-controlled power switch. Provides tools: power_on, power_off, power_status.",
)


@mcp.tool
def power_on_tool() -> str:
    """Power ON the RPi3B board."""
    log("power_on_tool called")
    result = power_on()
    log(f"power_on_tool result: {result}")
    return result


@mcp.tool
def power_off_tool() -> str:
    """Power OFF the RPi3B board."""
    log("power_off_tool called")
    result = power_off()
    log(f"power_off_tool result: {result}")
    return result


@mcp.tool
def power_status_tool() -> str:
    """Check power switch TTY status. Returns CH1:ON/OFF CH2:ON/OFF."""
    log("power_status_tool called")
    result = power_status()
    log(f"power_status_tool result: {result}")
    return result


if __name__ == "__main__":
    log("=== Power MCP server running ===")
    mcp.run()
