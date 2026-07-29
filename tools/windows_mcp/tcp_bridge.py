#!/usr/bin/env python3
"""TCP-to-stdio MCP bridge for connecting to Windows MCP servers.

This bridge runs on WSL2 and forwards MCP stdio protocol over TCP.
Windows MCP servers run on the Windows host, and this bridge connects to them.

Usage:
    python tcp_bridge.py --power-cmd "ssh windows-host python C:/path/to/power_mcp.py"
    python tcp_bridge.py --serial-cmd "ssh windows-host python C:/path/to/serial_mcp.py"

Or directly via Windows share:
    python tcp_bridge.py --power-cmd "cmd /c python C:\\path\\to\\power_mcp.py"
"""

import argparse
import asyncio
import json
import subprocess
import sys
import threading
import socket
import time


def read_until(stream, delimiter=b'\n'):
    """Read from stream until delimiter."""
    buf = b''
    while True:
        chunk = stream.read(1)
        if not chunk:
            break
        buf += chunk
        if delimiter in buf:
            return buf


def pipe_stdio_to_tcp(process, conn, log_prefix):
    """Pipe MCP stdio output to TCP connection."""
    try:
        while True:
            line = read_until(process.stdout, b'\n')
            if not line:
                break
            try:
                conn.sendall(line)
                log(f"{log_prefix} -> TCP: {line[:100]}")
            except:
                break
    except Exception as e:
        log(f"{log_prefix} pipe_stdio_to_tcp error: {e}")
    finally:
        try:
            conn.close()
        except:
            pass
        process.terminate()


def pipe_tcp_to_stdio(conn, process, log_prefix):
    """Pipe TCP input to MCP stdio."""
    try:
        while True:
            data = conn.recv(4096)
            if not data:
                break
            process.stdin.write(data)
            process.stdin.flush()
            log(f"{log_prefix} TCP -> stdin: {data[:100]}")
    except Exception as e:
        log(f"{log_prefix} pipe_tcp_to_stdio error: {e}")
    finally:
        process.terminate()


def handle_client(conn, addr, cmd, log_prefix):
    """Handle a single TCP client connection."""
    log(f"{log_prefix} Client connected from {addr}")

    proc = subprocess.Popen(
        cmd,
        shell=True,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    t1 = threading.Thread(target=pipe_stdio_to_tcp, args=(proc, conn, log_prefix), daemon=True)
    t2 = threading.Thread(target=pipe_tcp_to_stdio, args=(conn, proc, log_prefix), daemon=True)
    t1.start()
    t2.start()

    try:
        proc.wait()
    except:
        proc.terminate()
    log(f"{log_prefix} Client disconnected")


def start_server(host, port, cmd, log_prefix):
    """Start TCP server that forwards to MCP command."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((host, port))
    sock.listen(1)
    log(f"{log_prefix} Bridge listening on {host}:{port}")

    while True:
        try:
            conn, addr = sock.accept()
            handle_client(conn, addr, cmd, log_prefix)
        except Exception as e:
            log(f"{log_prefix} Accept error: {e}")
            time.sleep(1)


def log(msg):
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}", flush=True)


def main():
    parser = argparse.ArgumentParser(description="TCP-to-stdio MCP bridge")
    parser.add_argument("--power-host", default="127.0.0.1", help="Host for power MCP")
    parser.add_argument("--power-port", type=int, default=9001, help="Port for power MCP")
    parser.add_argument("--power-cmd", help="Command to run power MCP on Windows")

    parser.add_argument("--serial-host", default="127.0.0.1", help="Host for serial MCP")
    parser.add_argument("--serial-port", type=int, default=9002, help="Port for serial MCP")
    parser.add_argument("--serial-cmd", help="Command to run serial MCP on Windows")

    args = parser.parse_args()

    threads = []

    if args.power_cmd:
        t = threading.Thread(target=start_server, args=(args.power_host, args.power_port, args.power_cmd, "[POWER]"))
        t.daemon = True
        t.start()
        threads.append(t)
    else:
        log("[POWER] No command specified, skipping")

    if args.serial_cmd:
        t = threading.Thread(target=start_server, args=(args.serial_host, args.serial_port, args.serial_cmd, "[SERIAL]"))
        t.daemon = True
        t.start()
        threads.append(t)
    else:
        log("[SERIAL] No command specified, skipping")

    if not threads:
        log("No bridges configured, exiting")
        sys.exit(1)

    log("All bridges started, waiting...")
    for t in threads:
        t.join()


if __name__ == "__main__":
    main()
