#!/usr/bin/env python3
"""MCP server for reading RPi3 serial console on Windows.

Serial device on COM7 at 115200 baud.

Run directly as a script (uses stdio protocol):
  python serial_mcp.py
"""

import os
import re
import threading
import time

import serial
from serial.tools.list_ports import comports

from fastmcp import FastMCP

SERIAL_DEVICE = "COM7"
BAUD = 115200
LOG_FILE = "serial_mcp.log"


def log(msg):
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")


log("=== Serial MCP server starting ===")


class SerialBackend:
    def __init__(
        self,
        device=SERIAL_DEVICE,
        baudrate=BAUD,
        timeout=0.1,
    ):
        self.device = device
        self.baudrate = baudrate
        self.timeout = timeout

        self._lock = threading.Lock()
        self._buffer = bytearray()

        self._running = False
        self._thread = None
        self._serial = None

    def start(self):
        if self._running:
            return

        log(f"SerialBackend.start: opening {self.device} at {self.baudrate}")
        try:
            self._serial = serial.Serial(
                self.device,
                self.baudrate,
                timeout=self.timeout,
            )
            log(f"SerialBackend.start: opened successfully")
        except serial.SerialException as e:
            log(f"SerialBackend.start: initial open failed: {e}, will retry in reader loop")
            self._serial = None

        self._running = True

        self._thread = threading.Thread(
            target=self._reader_loop,
            daemon=True,
        )
        self._thread.start()

    def stop(self):
        self._running = False

        if self._thread:
            self._thread.join(timeout=1)

        if self._serial:
            self._serial.close()

    def _reader_loop(self):
        reconnect_delay = 1
        last_data_time = time.time()
        stale_threshold = 30

        while self._running:
            try:
                if self._serial is None or not self._serial.is_open:
                    log(f"_reader_loop: serial device not open, waiting {reconnect_delay}s to reconnect")
                    time.sleep(reconnect_delay)
                    reconnect_delay = min(reconnect_delay * 2, 30)
                    last_data_time = time.time()
                    try:
                        self._serial = serial.Serial(self.device, self.baudrate, timeout=self.timeout)
                        log(f"_reader_loop: reconnected to {self.device}")
                        reconnect_delay = 1
                    except Exception as e:
                        log(f"_reader_loop: reconnect failed: {e}")
                        continue

                waiting = self._serial.in_waiting

                if waiting:
                    data = self._serial.read(waiting)
                    with self._lock:
                        self._buffer.extend(data)
                    last_data_time = time.time()
                else:
                    if time.time() - last_data_time > stale_threshold:
                        log(f"_reader_loop: stale connection detected (no data for {stale_threshold}s), forcing reconnect")
                        try:
                            self._serial.close()
                        except:
                            pass
                        self._serial = None
                        last_data_time = time.time()
                        reconnect_delay = 1
                        continue
                    time.sleep(0.02)

            except Exception as e:
                log(f"_reader_loop: exception: {e}, will reconnect")
                if self._serial:
                    try:
                        self._serial.close()
                    except:
                        pass
                self._serial = None
                time.sleep(reconnect_delay)
                reconnect_delay = min(reconnect_delay * 2, 30)
                last_data_time = time.time()

    def clear(self):
        try:
            with self._lock:
                self._buffer.clear()
            log("clear: buffer cleared")
        except Exception as e:
            log(f"clear: exception: {e}")

    def read(self):
        try:
            with self._lock:
                data = bytes(self._buffer)
                self._buffer.clear()
            return data.decode(errors="replace")
        except Exception as e:
            log(f"read: exception: {e}")
            return ""

    def read_wait(self, timeout_ms=5000):
        max_wait_ms = min(timeout_ms, 5000)
        deadline = time.time() + max_wait_ms / 1000

        while time.time() < deadline:
            with self._lock:
                if self._buffer:
                    data = bytes(self._buffer)
                    self._buffer.clear()
                    return data.decode(errors="replace")

            try:
                if self._serial is not None and self._serial.is_open:
                    try:
                        waiting = self._serial.in_waiting
                        if waiting > 0:
                            data = self._serial.read(waiting)
                            with self._lock:
                                self._buffer.extend(data)
                    except (OSError, serial.SerialException) as e:
                        log(f"read_wait: serial read failed: {e}")
                        try:
                            self._serial.close()
                        except:
                            pass
                        self._serial = None
            except Exception as e:
                log(f"read_wait: serial access exception: {e}")
                self._serial = None

            time.sleep(0.05)

        with self._lock:
            if self._buffer:
                data = bytes(self._buffer)
                self._buffer.clear()
                return data.decode(errors="replace")

        return ""

    def write(self, text):
        try:
            if self._serial and self._serial.is_open:
                self._serial.write(text.encode())
                self._serial.flush()
                return "OK"
            else:
                log("write: serial not open")
                return "ERROR: serial not open"
        except Exception as e:
            log(f"write: exception: {e}")
            return f"ERROR: {e}"

    def is_open(self):
        return self._serial is not None and self._serial.is_open


backend = SerialBackend()


def check_device():
    available = [p.device for p in comports()]
    if backend.device not in available:
        return f"Device {backend.device} not found. Available: {', '.join(available)}"
    return f"Device {backend.device} found"


mcp = FastMCP(
    name="Serial MCP",
    instructions="MCP server for reading RPi3 serial console on COM7 at 115200 baud. Provides tools: serial_read, serial_write, serial_clear, serial_is_open, serial_wait, serial_reconnect.",
)


@mcp.tool
def serial_read(timeout_ms: int = 5000) -> str:
    """Read serial data from RPi3 console.

    Args:
        timeout_ms: Maximum wait time in milliseconds (max 300000, capped at 5000 internally)

    Returns:
        Serial data as string, or empty string if no data available
    """
    log(f"serial_read called: timeout_ms={timeout_ms}")
    if not backend.is_open():
        log("serial_read: serial not open, returning empty")
        return ""
    result = backend.read_wait(timeout_ms)
    log(f"serial_read result: '{result[:100]}...' ({len(result)} chars)" if len(result) > 100 else f"serial_read result: '{result}'")
    return result


@mcp.tool
def serial_write(text: str) -> str:
    """Send text to serial console.

    Args:
        text: Text to send

    Returns:
        OK or error message
    """
    log(f"serial_write called: {text[:50]}")
    return backend.write(text)


@mcp.tool
def serial_clear() -> str:
    """Clear serial buffer."""
    log("serial_clear called")
    backend.clear()
    return "OK"


@mcp.tool
def serial_is_open() -> bool:
    """Check if serial port is open."""
    result = backend.is_open()
    log(f"serial_is_open result: {result}")
    return result


@mcp.tool
def serial_wait(pattern: str, timeout_ms: int = 30000) -> str:
    """Wait for a regex pattern in serial output.

    Args:
        pattern: Regex pattern to wait for
        timeout_ms: Maximum wait time in milliseconds

    Returns:
        Matched text if found, empty string if timeout
    """
    log(f"serial_wait called: pattern={pattern}, timeout_ms={timeout_ms}")
    start = time.time()
    deadline = time.time() + timeout_ms / 1000
    accumulated = ""

    while time.time() < deadline:
        data = backend.read()
        if data:
            accumulated += data
            if re.search(pattern, accumulated):
                log(f"serial_wait: pattern '{pattern}' found")
                return accumulated
        time.sleep(0.1)

    log(f"serial_wait: timeout, pattern '{pattern}' not found")
    return accumulated


@mcp.tool
def serial_reconnect() -> str:
    """Reconnect serial port."""
    log("serial_reconnect called")
    if backend._serial:
        try:
            backend._serial.close()
        except:
            pass
        backend._serial = None
    try:
        backend._serial = serial.Serial(backend.device, backend.baudrate, timeout=backend.timeout)
        log("serial_reconnect: reconnected successfully")
        return "OK: reconnected"
    except Exception as e:
        log(f"serial_reconnect: failed: {e}")
        return f"ERROR: {e}"


if __name__ == "__main__":
    backend.start()
    log("=== Serial MCP server running ===")
    mcp.run()
