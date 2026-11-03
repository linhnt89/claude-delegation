#!/usr/bin/python3
"""Deterministic credential-free worker; installed only in the explicit private root."""
import json
import os
from pathlib import Path
import socket
import sys

home = Path(os.environ["HOME"])
records = home / ".cache/observer-worker"
records.mkdir(parents=True, exist_ok=True)
hints = {k: v for k, v in os.environ.items() if k.startswith(("HERDR_", "CLAUDE_", "CODEX_", "PI_", "GEMINI_", "OPENCODE_", "OMP_"))}
(records / (str(os.getpid()) + ".json")).write_text(json.dumps({"hints": hints, "stdin_eof": sys.stdin.read() == "", "environment_keys": sorted(os.environ), "delegate_home": os.environ.get("DELEGATE_HOME"), "sandboxed": os.environ.get("DELEGATE_SANDBOXED")}))
# Regression sentinel: if a caller accidentally forwards reporting hints, this
# stand-in would overwrite the selected pane identity through the fixture server.
if os.environ.get("HERDR_SOCKET_PATH") and os.environ.get("HERDR_PANE_ID"):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(os.environ["HERDR_SOCKET_PATH"])
        client.sendall((json.dumps({"id": "worker-sentinel", "method": "pane.report_agent_session", "params": {"pane_id": os.environ["HERDR_PANE_ID"], "source": "herdr:pi", "agent": "pi", "agent_session_id": "worker-must-not-report"}}) + "\n").encode())
        client.recv(4096)
Path(".task/REPORT.md").write_text("# Stand-in research report\nNo product changes; synthetic activity only.\n")
for n, (tool, args) in enumerate((("read", {"path": "synthetic.txt"}), ("bash", {"command": "echo synthetic"})), 1):
    print(json.dumps({"type": "tool_execution_start", "toolCallId": str(n), "toolName": tool, "args": args}), flush=True)
    print(json.dumps({"type": "tool_execution_end", "toolCallId": str(n), "isError": False}), flush=True)
