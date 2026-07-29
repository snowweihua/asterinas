#!/usr/bin/env python3
"""Persistent TCP-to-stdio relay for MCP servers.

This relay keeps MCP processes running persistently and multiplexes
multiple TCP client connections to the same process.

On Windows, run:
    python tcp_relay_persistent.py --power "python power_mcp.py" --serial "python serial_mcp.py"
"""

import subprocess
import threading
import argparse
import socket
import time
import sys


def log(msg):
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}", flush=True)


class MCPProcess:
    """Manages a persistent MCP process with multiple TCP client connections."""

    def __init__(self, cmd, log_prefix):
        self.cmd = cmd
        self.log_prefix = log_prefix
        self.proc = None
        self.lock = threading.Lock()
        self.clients = set()
        self.start()

    def start(self):
        log(f"{self.log_prefix} Starting MCP process: {self.cmd}")
        self.proc = subprocess.Popen(
            self.cmd,
            shell=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=0,
        )
        threading.Thread(target=self._reader_loop, daemon=True).start()

    def _reader_loop(self):
        """Read from MCP stdout and broadcast to all clients."""
        try:
            while True:
                if self.proc.stdout is None:
                    break
                chunk = self.proc.stdout.read(1)
                if not chunk:
                    log(f"{self.log_prefix} MCP process exited")
                    break
                with self.lock:
                    dead_clients = set()
                    for client in self.clients:
                        try:
                            client.sendall(chunk)
                        except:
                            dead_clients.add(client)
                    for client in dead_clients:
                        self.clients.remove(client)
        except Exception as e:
            log(f"{self.log_prefix} Reader error: {e}")

    def write(self, data):
        """Write data to MCP stdin from any client."""
        with self.lock:
            if self.proc and self.proc.stdin:
                try:
                    self.proc.stdin.write(data)
                    self.proc.stdin.flush()
                except:
                    pass

    def add_client(self, client_sock):
        with self.lock:
            self.clients.add(client_sock)

    def remove_client(self, client_sock):
        with self.lock:
            self.clients.discard(client_sock)

    def restart_if_dead(self):
        if self.proc and self.proc.poll() is not None:
            log(f"{self.log_prefix} MCP process died, restarting...")
            self.start()


def handle_client(conn, mcp_process, log_prefix):
    """Handle a single TCP client connection."""
    log(f"{log_prefix} Client connected")
    mcp_process.add_client(conn)

    try:
        while True:
            data = conn.recv(1024)
            if not data:
                break
            mcp_process.write(data)
    except:
        pass
    finally:
        mcp_process.remove_client(conn)
        try:
            conn.close()
        except:
            pass
        log(f"{log_prefix} Client disconnected")


def start_server(host, port, cmd, log_prefix):
    """Start TCP server that forwards to MCP process."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        sock.bind((host, port))
    except OSError as e:
        log(f"{log_prefix} Failed to bind {host}:{port}: {e}")
        return None

    sock.listen(5)
    log(f"{log_prefix} Server listening on {host}:{port}")

    mcp = MCPProcess(cmd, log_prefix)

    # Monitor thread to restart MCP if it dies
    def monitor():
        while True:
            time.sleep(5)
            mcp.restart_if_dead()

    threading.Thread(target=monitor, daemon=True).start()

    while True:
        try:
            conn, addr = sock.accept()
            handle_client(conn, mcp, log_prefix)
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
