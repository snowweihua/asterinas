#!/usr/bin/env python3
"""MCP stdio bridge - acts as an MCP server via stdio, proxies to HTTP MCP servers."""

import json
import sys
import requests

MCP_SERVERS = {
    "build-mcp": "http://localhost:8912/",
    "serial-mcp": "http://localhost:8910/",
}

def main():
    url = MCP_SERVERS.get("build-mcp", "http://localhost:8912/")
    session = requests.Session()
    session.headers.update({"Connection": "close"})

    while True:
        line = sys.stdin.readline()
        if not line:
            break

        try:
            req = json.loads(line.strip())
            method = req.get("method", "")
            req_id = req.get("id", 1)
            params = req.get("params", {})

            if method == "initialize":
                response = {
                    "jsonrpc": "2.0", "id": req_id, "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "build-mcp-bridge", "version": "1.0.0"}
                    }
                }
                print(json.dumps(response), flush=True)

            elif method == "tools/list":
                resp = session.post(url, json={"jsonrpc": "2.0", "method": "tools/list", "id": req_id}, timeout=5)
                result = resp.json().get("result", {})
                response = {"jsonrpc": "2.0", "id": req_id, "result": result}
                print(json.dumps(response), flush=True)

            elif method == "tools/call":
                tool_name = params.get("name", "")
                tool_args = params.get("arguments", {})
                resp = session.post(url, json={
                    "jsonrpc": "2.0", "method": "tools/call", "id": req_id,
                    "params": {"name": tool_name, "arguments": tool_args}
                }, timeout=3600)
                data = resp.json()
                response = {"jsonrpc": "2.0", "id": req_id, "result": data.get("result", {})}
                print(json.dumps(response), flush=True)

            elif method == "notifications/initialized":
                pass

        except Exception as e:
            error = {"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": str(e)}}
            print(json.dumps(error), flush=True)

if __name__ == "__main__":
    main()
