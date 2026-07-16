#!/bin/bash
# SPDX-License-Identifier: MPL-2.0

# Save session state to LATEST_SESSION.md before speckit commands
# This ensures debugging context is preserved across sessions

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FEATURE_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LATEST_SESSION="$FEATURE_DIR/specs/001-rpi3-hardware-bringup/LATEST_SESSION.md"

if [ ! -f "$LATEST_SESSION" ]; then
    echo "ERROR: $LATEST_SESSION not found"
    exit 1
fi

# Open editor for user to update the file
echo "Opening $LATEST_SESSION for review/update..."
echo "Please update:"
echo "  - Current issue and symptoms"
echo "  - Files modified this session"
echo "  - Next debugging step"
echo "  - Serial output if new boot test was done"
echo ""

# Try to use editor, fallback to cat
if [ -n "$EDITOR" ]; then
    $EDITOR "$LATEST_SESSION"
elif [ -n "$VISUAL" ]; then
    $VISUAL "$LATEST_SESSION"
else
    cat "$LATEST_SESSION"
    echo ""
    echo "No EDITOR set. Please manually update $LATEST_SESSION"
fi

echo ""
echo "Session state saved to: $LATEST_SESSION"
