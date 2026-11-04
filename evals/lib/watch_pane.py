#!/usr/bin/env python3
"""Run `delegate watch` as a Herdr side pane would, for checks.

    watch_pane.py <pty|pipe> <quit-flag> <result-file> -- <command...>

`pty` gives it a terminal and types `q` once <quit-flag> exists; `pipe` gives it a pipe
and sends SIGTERM instead. Either way, it waits up to 10 s for the exit, then writes
{"exit": code, "stopped_at": time, "exited_at": time, "killed": bool} to <result-file>.
Output goes to <result-file>.out. Stdin is never used for anything but the quit key.
"""
import json
import os
import pty
import signal
import subprocess
import sys
import time

mode, flag, result = sys.argv[1:4]
cmd = sys.argv[sys.argv.index("--") + 1:]
out = open(result + ".out", "wb")

if mode == "pty":
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp(cmd[0], cmd)
    os.set_blocking(fd, False)

    def drain():
        try:
            while True:
                chunk = os.read(fd, 65536)
                if not chunk:
                    break
                out.write(chunk)
        except (BlockingIOError, OSError):
            pass
else:
    proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT)
    pid = proc.pid

    def drain():
        pass


def exited():
    try:
        done, status = os.waitpid(pid, os.WNOHANG)
    except ChildProcessError:
        return 0
    if done == 0:
        return None
    return os.waitstatus_to_exitcode(status)


code = None
while not os.path.exists(flag):
    drain()
    code = exited()
    if code is not None:
        break
    time.sleep(0.05)
stopped_at = time.time()
killed = False
if code is None:
    if mode == "pty":
        os.write(fd, b"q")
    else:
        os.kill(pid, signal.SIGTERM)
    end = time.time() + 10
    while code is None and time.time() < end:
        drain()
        code = exited()
        time.sleep(0.05)
    if code is None:
        os.kill(pid, signal.SIGKILL)
        killed = True
        os.waitpid(pid, 0)
drain()
with open(result, "w", encoding="utf-8") as f:
    json.dump({"exit": code, "stopped_at": stopped_at, "exited_at": time.time(), "killed": killed}, f)
