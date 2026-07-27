#!/usr/bin/env python3
"""MCP stdio server - wraps build_mcp FastMCP server."""

import sys
sys.path.insert(0, "tools")

from build_mcp.server import mcp

mcp.run()
