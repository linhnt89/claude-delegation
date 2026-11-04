#!/usr/bin/env python3
"""Stand-in Herdr 0.9.5 socket for checks: pane.get, pane.report_metadata, pane.split, pane input.

    herdr_standin.py <socket> <panes.json> <requests.jsonl>

Each connection reads one newline-terminated JSON request and writes one response, like
Herdr's API (src/api/client.rs at 2ecf19a9). `panes.json` is re-read on every request, so
a check can change a pane mid-run. Each pane is keyed by its id:

    {"w1:p1": {"agent_session": {"source": "herdr:pi", "agent": "pi",
                                 "kind": "path", "value": "/home/u/s.jsonl"}},
     "w1:p2": {"mode": "error"}}

`pane.split` answers a new pane id (w1:p101, ...) with the requested cwd and focus;
`pane.send_input`/`send_text`/`send_keys` answer ok. Both only record: they run nothing.
`mode` (optional) misbehaves on `pane.get`: error | wrong_pane | wrong_id | slow |
oversize | garbage | close. `pane.report_metadata` updates that pane's tokens the way
Herdr 0.9.5 did in the lead's probe (docs/research/herdr-sidebar-tokens-probe.md):
values trimmed and cut to 80 chars, C0 controls stripped, empty or null clears, ttl_ms
expires keys. Tokens are kept in `<panes.json>.tokens.json`, rewritten after each change.
Every request is appended to requests.jsonl as {"at", "method", "params"}.
Unknown methods get an error envelope and are recorded too.
"""
import json
import os
import socket
import sys
import threading
import time

SOCK, PANES, LOG = sys.argv[1:4]
TOKENS = PANES + ".tokens.json"
lock = threading.Lock()
tokens = {}  # pane -> key -> (value, expires_at or None)
next_pane = 100  # ids handed out by pane.split


def load_panes():
    try:
        with open(PANES, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_tokens():
    now = time.time()
    view = {p: {k: v for k, (v, exp) in keys.items() if exp is None or exp > now}
            for p, keys in tokens.items()}
    tmp = TOKENS + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(view, f)
    os.replace(tmp, TOKENS)


def expiry_loop():
    while True:
        time.sleep(0.1)
        with lock:
            save_tokens()


def clean(value):
    if value is None:
        return None
    text = "".join(ch for ch in str(value).strip() if not (ord(ch) < 32 or ord(ch) == 127))
    text = text[:80].strip()
    return text or None


def envelope(rid, result=None, error=None):
    body = {"id": rid}
    if error is not None:
        body["error"] = {"code": error[0], "message": error[1]}
    else:
        body["result"] = result
    return (json.dumps(body) + "\n").encode()


def handle(conn):
    try:
        conn.settimeout(5)
        data = b""
        while not data.endswith(b"\n") and len(data) < 4 << 20:
            chunk = conn.recv(65536)
            if not chunk:
                break
            data += chunk
        try:
            req = json.loads(data)
        except ValueError:
            return
        rid, method, params = req.get("id"), req.get("method"), req.get("params") or {}
        with lock, open(LOG, "a", encoding="utf-8") as log:
            log.write(json.dumps({"at": time.time(), "method": method, "params": params}) + "\n")
        panes = load_panes()
        pid = params.get("pane_id")
        pane = panes.get(pid) if isinstance(pid, str) else None
        if method == "pane.get":
            if pane is None:
                conn.sendall(envelope(rid, error=("pane_not_found", f"pane {pid} not found")))
                return
            mode = pane.get("mode")
            if mode == "close":
                return
            if mode == "slow":
                time.sleep(6)
            if mode == "garbage":
                conn.sendall(b"not json\n")
                return
            if mode == "oversize":
                conn.sendall(b'{"id":"' + b"x" * (3 << 20) + b'"}\n')
                return
            if mode == "error":
                conn.sendall(envelope(rid, error=("internal", "stand-in error")))
                return
            info = {"pane_id": "w9:p9" if mode == "wrong_pane" else pid,
                    "workspace_id": pid.split(":")[0], "agent_status": "unknown"}
            if pane.get("agent_session") is not None:
                info["agent_session"] = pane["agent_session"]
            with lock:
                live = {k: v for k, (v, exp) in tokens.get(pid, {}).items()
                        if exp is None or exp > time.time()}
            if live:
                info["tokens"] = live
            rid_out = "other-request" if mode == "wrong_id" else rid
            conn.sendall(envelope(rid_out, {"type": "pane_info", "pane": info}))
        elif method == "pane.report_metadata":
            if pane is None:
                conn.sendall(envelope(rid, error=("pane_not_found", f"pane {pid} not found")))
                return
            patch = params.get("tokens") or {}
            ttl = params.get("ttl_ms")
            with lock:
                keys = tokens.setdefault(pid, {})
                for key, value in patch.items():
                    value = clean(value)
                    if value is None:
                        keys.pop(key, None)
                    else:
                        exp = time.time() + ttl / 1000 if isinstance(ttl, (int, float)) else None
                        keys[key] = (value, exp)
                save_tokens()
            conn.sendall(envelope(rid, {"type": "ok"}))
        elif method == "pane.split":
            target = params.get("target_pane_id")
            if target not in panes:
                conn.sendall(envelope(rid, error=("pane_not_found", f"pane {target} not found")))
                return
            with lock:
                global next_pane
                next_pane += 1
                new_id = f"{target.split(':')[0]}:p{next_pane}"
            info = {"pane_id": new_id, "workspace_id": target.split(":")[0],
                    "agent_status": "unknown", "cwd": params.get("cwd"),
                    "focused": bool(params.get("focus"))}
            conn.sendall(envelope(rid, {"type": "pane_info", "pane": info}))
        elif method in ("pane.send_input", "pane.send_text", "pane.send_keys"):
            conn.sendall(envelope(rid, {"type": "ok"}))
        else:
            conn.sendall(envelope(rid, error=("unknown_method", f"unknown method {method}")))
    except OSError:
        pass
    finally:
        conn.close()


def main():
    if os.path.exists(SOCK):
        os.unlink(SOCK)
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCK)
    srv.listen(64)
    with lock:
        save_tokens()
    threading.Thread(target=expiry_loop, daemon=True).start()
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=handle, args=(conn,), daemon=True).start()


if __name__ == "__main__":
    main()
