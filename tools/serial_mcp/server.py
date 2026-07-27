#!/usr/bin/env python3
"""MCP server for reading RPi3 serial console.

Run directly as a script (uses stdio protocol):
  python3 -m serial_mcp.server
"""

import re
import threading
import time

import serial

from fastmcp import FastMCP

SERIAL_DEVICE = "/dev/ttyUSB0"
BAUD = 115200


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

        self._serial = serial.Serial(
            self.device,
            self.baudrate,
            timeout=self.timeout,
        )

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
        while self._running:
            try:
                waiting = self._serial.in_waiting

                if waiting:
                    data = self._serial.read(waiting)
                    with self._lock:
                        self._buffer.extend(data)

                else:
                    time.sleep(0.02)

            except Exception:
                time.sleep(0.1)

    def clear(self):
        with self._lock:
            self._buffer.clear()

    def read(self):
        with self._lock:
            data = bytes(self._buffer)
            self._buffer.clear()

        return data.decode(errors="replace")

    def read_wait(self, timeout_ms=5000):
        deadline = time.time() + timeout_ms / 1000

        while time.time() < deadline:
            with self._lock:
                if self._buffer:
                    data = bytes(self._buffer)
                    self._buffer.clear()
                    return data.decode(errors="replace")

            time.sleep(0.05)

        return ""

    def write(self, text):
        self._serial.write(text.encode())

    def wait_for(self, pattern, timeout_ms=30000):
        regex = re.compile(pattern)

        deadline = time.time() + timeout_ms / 1000

        while time.time() < deadline:

            with self._lock:
                text = self._buffer.decode(errors="replace")

            if regex.search(text):
                return text

            time.sleep(0.05)

        raise TimeoutError(f"Pattern not found: {pattern}")

    def capture(self, seconds):
        self.clear()

        time.sleep(seconds)

        return self.read()

    def reconnect(self):
        self.stop()
        time.sleep(1)
        self.start()
        return True

    def is_open(self):
        return self._serial is not None and self._serial.is_open


backend = SerialBackend()
backend.start()


mcp = FastMCP(
    name="RPi Serial",
    instructions="MCP server for communicating with Raspberry Pi through UART. Provides tools: serial_read, serial_write, serial_wait, serial_clear, serial_capture, serial_is_open, serial_reconnect.",
)


@mcp.tool
def serial_read(timeout_ms: int = 5000) -> str:
    """Read accumulated serial output. Waits until data is available or timeout expires."""
    return backend.read_wait(timeout_ms)


@mcp.tool
def serial_write(text: str) -> str:
    """Send text to the serial port."""
    backend.write(text)
    return "OK"


@mcp.tool
def serial_wait(pattern: str, timeout_ms: int = 30000) -> str:
    """Wait until regex pattern appears in serial output."""
    return backend.wait_for(pattern, timeout_ms)


@mcp.tool
def serial_clear() -> str:
    """Clear buffered serial output."""
    backend.clear()
    return "Buffer cleared."


@mcp.tool
def serial_capture(seconds: int = 5) -> str:
    """Capture serial output for a period of time."""
    return backend.capture(seconds)


@mcp.tool
def serial_is_open() -> bool:
    """Check whether the serial port is connected."""
    return backend.is_open()


@mcp.tool
def serial_reconnect() -> str:
    """Reopen the serial port."""
    backend.reconnect()
    return "Reconnected."


if __name__ == "__main__":
    mcp.run()
