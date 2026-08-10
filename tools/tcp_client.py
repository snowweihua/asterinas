#!/usr/bin/env python3
"""TCP-to-stdio client that connects to Windows MCP relay.

This runs in WSL2 and connects to the tcp_relay_persistent.py running on Windows.
It handles MCP handshake locally and forwards tools/call to the relay.

Usage:
    python tcp_client.py --host 172.19.96.1 --port 9001

Then OpenCode talks to this process via stdio.
"""

import os
import socket
import sys
import threading
import time


def _is_duplicate_instance():
    """Avoid duplicate MCP clients when devin spawns both 'devin list' and 'devin acp'.

    The active agent is the 'devin acp' child.  If our parent (or a sibling under the
    same parent) is/contains a 'devin acp' process, any sibling 'devin list' instance
    should exit so only one TCP client per port stays connected to the relay.
    """
    try:
        ppid = os.getppid()
        # Read the parent's command line.
        with open(f"/proc/{ppid}/cmdline", "rb") as f:
            parent_cmd = f.read().replace(b"\0", b" ").decode("ascii", "replace")
        # If we are directly started by devin acp, keep running.
        if "devin" in parent_cmd and "acp" in parent_cmd:
            return False
        # If the parent is devin list (or another devin wrapper) and it has a child
        # named devin acp, we are the duplicate wrapper instance.
        if "devin" in parent_cmd:
            children_path = f"/proc/{ppid}/task/{ppid}/children"
            if os.path.exists(children_path):
                with open(children_path, "r") as f:
                    for cpid in f.read().split():
                        try:
                            with open(f"/proc/{cpid}/cmdline", "rb") as cf:
                                ccmd = cf.read().replace(b"\0", b" ").decode("ascii", "replace")
                            if "devin" in ccmd and "acp" in ccmd:
                                return True
                        except FileNotFoundError:
                            continue
            # Also give devin acp a moment to appear before deciding.
            for _ in range(20):
                time.sleep(0.1)
                try:
                    with open(children_path, "r") as f:
                        for cpid in f.read().split():
                            try:
                                with open(f"/proc/{cpid}/cmdline", "rb") as cf:
                                    ccmd = cf.read().replace(b"\0", b" ").decode("ascii", "replace")
                                if "devin" in ccmd and "acp" in ccmd:
                                    return True
                            except FileNotFoundError:
                                continue
                except FileNotFoundError:
                    break
    except Exception as e:
        # If we cannot determine the process tree, run normally.
        print(f"[{os.path.basename(__file__)}] process-tree check failed: {e}", file=sys.stderr)
    return False


def log(msg):
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}", file=sys.stderr, flush=True)


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
    global sock

    if _is_duplicate_instance():
        log("Detected duplicate devin list MCP client; exiting.")
        sys.exit(0)

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

    for line in sys.stdin:
        if not line.strip():
            continue

        try:
            with lock:
                sock.sendall(line.encode())
        except Exception as e:
            log(f"Error: {e}")

    sock.close()


if __name__ == "__main__":
    main()
