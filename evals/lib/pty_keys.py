#!/usr/bin/env python3
"""Drive a terminal program with keys, for checks.

    pty_keys.py <out-dir> <script.json> -- <command...>

Runs the command in a pseudo-terminal (100x40) and follows the script, a list of steps:
  {"wait": seconds}            let it run
  {"key": "down"|"up"|"enter"|"tab"|"q"|"<text>"}   type a key
  {"snap": "<name>"}           save the output since the previous snap to <name>.raw
  {"flag": "<path>"}           wait (up to 30 s) until a file exists
Writes result.json: {"exit", "killed", "restored": bool, "lflag": int}. "restored" says
echo and canonical mode were back on the terminal after the program exited.
"""
import fcntl
import json
import os
import pty
import struct
import sys
import termios
import time

out_dir, script = sys.argv[1], json.load(open(sys.argv[2]))
cmd = sys.argv[sys.argv.index("--") + 1:]
KEYS = {"down": b"\x1b[B", "up": b"\x1b[A", "enter": b"\r", "tab": b"\t", "q": b"q"}

pid, fd = pty.fork()
if pid == 0:
    os.execvp(cmd[0], cmd)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 100, 0, 0))
os.set_blocking(fd, False)
buf = bytearray()
code = None


def pump(seconds):
    global code
    end = time.time() + seconds
    while time.time() < end:
        try:
            chunk = os.read(fd, 65536)
            if chunk:
                buf.extend(chunk)
                continue
        except (BlockingIOError, OSError):
            pass
        if code is None:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                code = os.waitstatus_to_exitcode(status)
        time.sleep(0.03)


for step in script:
    if "wait" in step:
        pump(step["wait"])
    elif "key" in step:
        os.write(fd, KEYS.get(step["key"], step["key"].encode()))
        pump(0.1)
    elif "snap" in step:
        pump(0.05)
        with open(os.path.join(out_dir, step["snap"] + ".raw"), "wb") as f:
            f.write(bytes(buf))
        buf.clear()
    elif "flag" in step:
        end = time.time() + 30
        while not os.path.exists(step["flag"]) and time.time() < end:
            pump(0.1)

end = time.time() + 10
while code is None and time.time() < end:
    pump(0.1)
killed = False
if code is None:
    os.kill(pid, 9)
    os.waitpid(pid, 0)
    killed = True
try:
    lflag = termios.tcgetattr(fd)[3]
except termios.error:
    lflag = -1
with open(os.path.join(out_dir, "rest.raw"), "wb") as f:
    f.write(bytes(buf))
restored = lflag >= 0 and bool(lflag & termios.ECHO) and bool(lflag & termios.ICANON)
json.dump({"exit": code, "killed": killed, "restored": restored, "lflag": lflag},
          open(os.path.join(out_dir, "result.json"), "w"))
