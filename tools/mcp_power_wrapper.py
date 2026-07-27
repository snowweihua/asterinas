#!/usr/bin/env python3
"""MCP stdio server - wraps power_mcp FastMCP server."""

import sys
sys.path.insert(0, "tools")

from power_mcp.server import mcp

mcp.run()
