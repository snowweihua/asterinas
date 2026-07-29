#!/usr/bin/env python3
"""MCP server for controlling RPi3B power via USB-controlled power switch on Windows.

The power switch uses a simple serial protocol on COM3:
  - Power ON:  0xA0 0x01 0x01 0xA2  then  0xA0 0x02 0x01 0xA3
  - Power OFF: 0xA0 0x01 0x00 0xA1  then  0xA0 0x02 0x00 0xA2
  - Status:    0xA0 0x01 0x02 0xA3  then  0xA0 0x02 0x02 0xA4 (returns CH1:ON/OFF CH2:ON/OFF)

Run directly as a script (uses stdio protocol):
  python power_mcp.py
"""

import os
import time
import serial
from serial.tools.list_ports import comports

from fastmcp import FastMCP

TTY_DEVICE = "COM3"
LOG_FILE = "power_mcp.log"


def log(msg):
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")


log("=== Power MCP server starting ===")


def wait_for_device(port, timeout=5.0):
    start = time.time()
    while time.time() - start < timeout:
        try:
            ser = serial.Serial(port, 115200, timeout=0.1)
            ser.close()
            return True
        except (serial.SerialException, OSError):
            pass
        time.sleep(0.2)
    return False


def send_raw_bytes(port, data: bytes):
    if not wait_for_device(port):
        raise OSError(f"Device {port} not available")
    ser = serial.Serial(port, 115200, timeout=1)
    try:
        ser.write(data)
        ser.flush()
        time.sleep(1)
    finally:
        ser.close()


ON_SEQ = [b"\xa0\x01\x01\xa2", b"\xa0\x02\x01\xa3"]
OFF_SEQ = [b"\xa0\x01\x00\xa1", b"\xa0\x02\x00\xa2"]
STATUS_SEQ = [b"\xa0\x01\x02\xa3", b"\xa0\x02\x02\xa4"]


def power_on() -> str:
    for cmd in ON_SEQ:
        send_raw_bytes(TTY_DEVICE, cmd)
    return "OK: power ON"


def power_off() -> str:
    for cmd in OFF_SEQ:
        send_raw_bytes(TTY_DEVICE, cmd)
    return "OK: power OFF"


def power_status() -> str:
    available_ports = [p.device for p in comports()]
    if TTY_DEVICE not in available_ports:
        return "ABSENT"

    try:
        ser = serial.Serial(TTY_DEVICE, 115200, timeout=1)
    except serial.SerialException as e:
        return f"PRESENT (error: {e})"

    try:
        ser.flushInput()
        responses = []
        for cmd in STATUS_SEQ:
            ser.write(cmd)
            time.sleep(0.3)
            d = ser.read(100)
            if d:
                responses.append(d.decode("utf-8", errors="replace").strip())

        if responses:
            return f"PRESENT | {' '.join(responses)}"
        return "PRESENT (no response)"
    finally:
        ser.close()


mcp = FastMCP(
    name="Power MCP",
    instructions="MCP server for controlling RPi3B power via USB-controlled power switch on COM3. Provides tools: power_on, power_off, power_status.",
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
    """Check power switch status. Returns CH1:ON/OFF CH2:ON/OFF."""
    log("power_status_tool called")
    result = power_status()
    log(f"power_status_tool result: {result}")
    return result


if __name__ == "__main__":
    log("=== Power MCP server running ===")
    mcp.run()
