#!/usr/bin/env python3
"""TCP-to-stdio relay for MCP servers.

Each TCP client gets an isolated MCP subprocess and stdio session.

On Windows, run:
    python tcp_relay_persistent.py --power "python power_mcp.py" --serial "python serial_mcp.py"
"""

import argparse
import os
import socket
import subprocess
import sys
import threading
import time


def log(msg):
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}", flush=True)


class ClientSession:
    """Owns one MCP subprocess and one TCP client connection."""

    def __init__(self, conn, cmd, log_prefix):
        self.conn = conn
        self.cmd = cmd
        self.log_prefix = log_prefix
        self.send_lock = threading.Lock()
        self.proc = None
        self.reader_thread = None

    def start(self):
        log(f"{self.log_prefix} Starting MCP process for client: {self.cmd}")
        creationflags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        self.proc = subprocess.Popen(
            self.cmd,
            shell=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
            creationflags=creationflags,
        )
        self.reader_thread = threading.Thread(target=self._reader_loop, daemon=True)
        self.reader_thread.start()

    def _reader_loop(self):
        try:
            while self.proc and self.proc.stdout:
                chunk = self.proc.stdout.read(1)
                if not chunk:
                    break
                try:
                    with self.send_lock:
                        self.conn.sendall(chunk)
                except OSError:
                    break
        except Exception as e:
            log(f"{self.log_prefix} Client reader error: {e}")
        finally:
            log(f"{self.log_prefix} MCP process exited for client")

    def write(self, data):
        if not self.proc or not self.proc.stdin or self.proc.poll() is not None:
            return False
        try:
            self.proc.stdin.write(data)
            self.proc.stdin.flush()
            return True
        except (BrokenPipeError, OSError):
            return False

    def close(self):
        if not self.proc:
            return
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
        except OSError:
            pass

        if self.proc.poll() is None:
            try:
                if os.name == "nt":
                    subprocess.run(
                        ["taskkill", "/PID", str(self.proc.pid), "/T", "/F"],
                        capture_output=True,
                        timeout=5,
                        check=False,
                    )
                else:
                    self.proc.terminate()
                self.proc.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                try:
                    self.proc.kill()
                    self.proc.wait(timeout=5)
                except (OSError, subprocess.TimeoutExpired):
                    pass

        if self.reader_thread and self.reader_thread is not threading.current_thread():
            self.reader_thread.join(timeout=1)


def handle_client(conn, addr, cmd, log_prefix):
    """Handle one TCP client with an isolated MCP process."""
    log(f"{log_prefix} Client connected: {addr}")
    session = ClientSession(conn, cmd, log_prefix)

    try:
        session.start()
        while True:
            data = conn.recv(1024)
            if not data or not session.write(data):
                break
    except OSError as e:
        log(f"{log_prefix} Client error {addr}: {e}")
    finally:
        session.close()
        try:
            conn.close()
        except OSError:
            pass
        log(f"{log_prefix} Client disconnected: {addr}")


def start_server(host, port, cmd, log_prefix):
    """Start a TCP server that creates one MCP process per client."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        sock.bind((host, port))
    except OSError as e:
        log(f"{log_prefix} Failed to bind {host}:{port}: {e}")
        return None

    sock.listen(5)
    log(f"{log_prefix} Server listening on {host}:{port}")

    while True:
        try:
            conn, addr = sock.accept()
            thread = threading.Thread(
                target=handle_client,
                args=(conn, addr, cmd, log_prefix),
                daemon=True,
            )
            thread.start()
        except Exception as e:
            log(f"{log_prefix} Accept error: {e}")
            time.sleep(1)


def main():
    parser = argparse.ArgumentParser(description="Persistent TCP-to-stdio MCP relay")
    parser.add_argument("--power-host", default="0.0.0.0", help="Host for power MCP")
    parser.add_argument("--power-port", type=int, default=9001, help="Port for power MCP")
    parser.add_argument("--power-cmd", help="Command to run power MCP, e.g., 'python power_mcp.py'")

    parser.add_argument("--serial-host", default="0.0.0.0", help="Host for serial MCP")
    parser.add_argument("--serial-port", type=int, default=9002, help="Port for serial MCP")
    parser.add_argument("--serial-cmd", help="Command to run serial MCP")

    args = parser.parse_args()

    threads = []

    if args.power_cmd:
        t = threading.Thread(target=start_server, args=(args.power_host, args.power_port, args.power_cmd, "[POWER]"))
        t.daemon = True
        t.start()
        threads.append(t)
        log(f"[POWER] Will run: {args.power_cmd}")
    else:
        log("[POWER] No command specified, skipping")

    if args.serial_cmd:
        t = threading.Thread(target=start_server, args=(args.serial_host, args.serial_port, args.serial_cmd, "[SERIAL]"))
        t.daemon = True
        t.start()
        threads.append(t)
        log(f"[SERIAL] Will run: {args.serial_cmd}")
    else:
        log("[SERIAL] No command specified, skipping")

    if not threads:
        log("No servers configured, exiting")
        sys.exit(1)

    log("All servers started, press Ctrl+C to stop...")
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        log("Shutting down...")


if __name__ == "__main__":
    main()
