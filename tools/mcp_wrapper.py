import sys
import subprocess
import time

# Get the target module name from arguments (e.g., build_mcp.server)
if len(sys.argv) < 2:
    sys.exit(1)

module_name = sys.argv[1]

while True:
    try:
        # Launch the underlying MCP server as a subprocess
        # Standard input and output are preserved to pass messages transparently
        process = subprocess.Popen(
            ["python3", "-m", module_name],
            stdin=sys.stdin,
            stdout=sys.stdout,
            stderr=sys.stderr
        )
        
        # Wait for the process to finish or crash
        process.wait()
    except Exception:
        pass
    
    # Optional cool-down period before restarting to prevent high CPU usage on loop loops
    time.sleep(1)

