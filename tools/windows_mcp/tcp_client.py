#!/usr/bin/env python3
"""TCP-to-stdio client that connects to Windows MCP relay.

This runs in WSL2 and connects to the tcp_relay.py running on Windows.
It pipes stdin/stdout between OpenCode (stdio) and the Windows TCP relay.

Usage:
    python tcp_client.py --host 172.19.96.1 --port 9001

Then OpenCode talks to this process via stdio.
"""

import socket
import sys
import threading
import time


def main():
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

    print(f"Connecting to {host}:{port}...", file=sys.stderr, flush=True)

    sock = None
    for attempt in range(5):
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect((host, port))
            break
        except (ConnectionRefused, OSError) as e:
            print(f"Connection attempt {attempt + 1} failed: {e}", file=sys.stderr)
            if sock:
                sock.close()
            sock = None
            time.sleep(2)

    if not sock:
        print("Failed to connect after 5 attempts", file=sys.stderr)
        sys.exit(1)

    print(f"Connected to {host}:{port}", file=sys.stderr, flush=True)

    def forward_tcp_to_stdout(sock, stdout):
        try:
            while True:
                data = sock.recv(4096)
                if not data:
                    break
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
        except:
            pass

    def forward_stdin_to_tcp(sock, stdin):
        try:
            while True:
                data = sys.stdin.buffer.read(1)
                if not data:
                    break
                sock.sendall(data)
        except:
            pass

    t1 = threading.Thread(target=forward_tcp_to_stdout, args=(sock, sys.stdout.buffer), daemon=True)
    t1.start()

    t2 = threading.Thread(target=forward_stdin_to_tcp, args=(sock, sys.stdin.buffer), daemon=True)
    t2.start()

    try:
        t1.join()
        t2.join()
    except KeyboardInterrupt:
        pass
    finally:
        sock.close()


if __name__ == "__main__":
    main()
