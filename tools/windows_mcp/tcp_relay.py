#!/usr/bin/env python3
"""TCP-to-stdio relay that exposes MCP servers on TCP ports.

On Windows, run:
    python tcp_relay.py --power "python power_mcp.py" --serial "python serial_mcp.py"

From WSL2, connect to Windows host on ports 9001 (power) and 9002 (serial).
"""

import subprocess
import threading
import argparse
import socket
import time
import sys


def log(msg):
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}", flush=True)


def handle_client(conn, cmd, log_prefix):
    """Handle a single TCP client - relay data between TCP and MCP stdio."""
    log(f"{log_prefix} Client connected")

    proc = None
    try:
        proc = subprocess.Popen(
            cmd,
            shell=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=0,
        )

        def forward_stdout():
            try:
                while True:
                    chunk = proc.stdout.read(1)
                    if not chunk:
                        break
                    try:
                        conn.sendall(chunk)
                    except:
                        break
            except:
                pass
            finally:
                try:
                    conn.shutdown(socket.SHUT_RDWR)
                except:
                    pass

        def forward_stdin():
            try:
                while True:
                    data = conn.recv(1024)
                    if not data:
                        break
                    proc.stdin.write(data)
                    proc.stdin.flush()
            except:
                pass
            finally:
                try:
                    proc.stdin.close()
                except:
                    pass

        t1 = threading.Thread(target=forward_stdout, daemon=True)
        t1.start()
        t2 = threading.Thread(target=forward_stdin, daemon=True)
        t2.start()

        proc.wait()

    except Exception as e:
        log(f"{log_prefix} Error: {e}")
    finally:
        try:
            conn.close()
        except:
            pass
        if proc and proc.poll() is None:
            proc.terminate()
        log(f"{log_prefix} Client disconnected")


def start_server(host, port, cmd, log_prefix):
    """Start TCP server that forwards to MCP command."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        sock.bind((host, port))
    except OSError as e:
        log(f"{log_prefix} Failed to bind {host}:{port}: {e}")
        return

    sock.listen(1)
    log(f"{log_prefix} Server listening on {host}:{port}")

    while True:
        try:
            conn, addr = sock.accept()
            handle_client(conn, cmd, log_prefix)
        except Exception as e:
            log(f"{log_prefix} Accept error: {e}")
            time.sleep(1)


def main():
    parser = argparse.ArgumentParser(description="TCP-to-stdio MCP relay")
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
