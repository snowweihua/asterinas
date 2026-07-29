#!/usr/bin/env python3
"""TCP-to-stdio client that connects to Windows MCP relay.

This runs in WSL2 and connects to the tcp_relay_persistent.py running on Windows.
It handles MCP handshake locally and forwards tools/call to the relay.

Usage:
    python tcp_client.py --host 172.19.96.1 --port 9001

Then OpenCode talks to this process via stdio.
"""

import json
import socket
import sys
import threading
import time


def log(msg):
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}", file=sys.stderr, flush=True)


initialized = False
sock = None
lock = threading.Lock()


def forward_tcp_to_stdout():
    """Forward data from TCP to stdout."""
    global sock
    try:
        while True:
            data = sock.recv(4096)
            if not data:
                break
            sys.stdout.buffer.write(data)
            sys.stdout.buffer.flush()
    except:
        pass


def main():
    global sock, initialized

    if len(sys.argv) < 4:
        print(f"Usage: {sys.argv[0]} --host HOST --port PORT", file=sys.stderr)
        sys.exit(1)

    host = None
    port = None

    i = 1
    while i < len(sys.argv):
        if sys.argv[i] == "--host" and i + 1 < len(sys.argv):
            host = sys.argv[i + 1]
            i += 2
        elif sys.argv[i] == "--port" and i + 1 < len(sys.argv):
            port = int(sys.argv[i + 1])
            i += 2
        else:
            i += 1

    if not host or not port:
        print("Error: --host and --port required", file=sys.stderr)
        sys.exit(1)

    log(f"Connecting to {host}:{port}...")

    for attempt in range(10):
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect((host, port))
            break
        except (ConnectionRefused, OSError) as e:
            log(f"Connection attempt {attempt + 1} failed: {e}")
            sock = None
            time.sleep(2)

    if not sock:
        log("Failed to connect after 10 attempts")
        sys.exit(1)

    log(f"Connected to {host}:{port}")

    t1 = threading.Thread(target=forward_tcp_to_stdout, daemon=True)
    t1.start()

    pending_responses = {}
    req_id = 1

    for line in sys.stdin:
        if not line.strip():
            continue

        try:
            req = json.loads(line)
            method = req.get("method", "")
            req_id = req.get("id", None)

            if method == "initialize":
                response = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "tcp-bridge", "version": "1.0.0"}
                    }
                }
                print(json.dumps(response), flush=True)
                initialized = True

                time.sleep(0.1)
                notify_init = {
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                }
                sock.sendall((json.dumps(notify_init) + "\n").encode())
                continue

            if method == "notifications/initialized":
                continue

            if not initialized:
                if req_id is not None:
                    fallback = json.dumps({"jsonrpc": "2.0", "id": req_id, "error": {"code": -32002, "message": "Server initializing..."}})
                    print(fallback, flush=True)
                continue

            if method == "tools/call":
                with lock:
                    sock.sendall(line.encode())
            else:
                with lock:
                    sock.sendall(line.encode())

        except json.JSONDecodeError:
            with lock:
                sock.sendall(line.encode())
        except Exception as e:
            log(f"Error: {e}")
            if req_id is not None:
                error = {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32603, "message": str(e)}}
                print(json.dumps(error), flush=True)

    sock.close()


if __name__ == "__main__":
    main()
