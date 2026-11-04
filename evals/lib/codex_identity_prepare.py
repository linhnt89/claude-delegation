"""Offline-safe preparation for S146; no lifecycle or evidence ownership."""

import hashlib
import json
import os
from pathlib import Path
import re
import select
import socket
import stat
import time


class PreparationError(Exception):
    """A redacted, stable setup failure."""

    def __init__(self, code):
        self.code = code
        super().__init__(code)


def _fail(code="unsafe_artifact"):
    raise PreparationError(code) from None


def _parts(path):
    path = os.fspath(path)
    if not isinstance(path, str) or not path.startswith("/") or "\x00" in path:
        _fail("invalid_path")
    parts = path.split("/")[1:]
    if not parts or any(p in ("", ".", "..") for p in parts):
        _fail("invalid_path")
    return parts


def _directory(path):
    """Walk every component without following links (including ancestors)."""
    parts = [] if os.fspath(path) == "/" else _parts(path)
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in parts:
            nxt = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                          dir_fd=fd)
            try:
                if _identity(os.fstat(nxt))[:2] != _identity(os.stat(
                        part, dir_fd=fd, follow_symlinks=False))[:2]:
                    _fail("artifact_drift")
            except BaseException:
                os.close(nxt)
                raise
            os.close(fd)
            fd = nxt
        return fd
    except BaseException:
        os.close(fd)
        raise


def _identity(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _regular(info, *, executable=False, private=False, no_shared_write=False,
             source_binary=False):
    # Explicit pinned source binaries may be system-owned or hardlinked; private
    # run artifacts must remain exclusively owned with exactly one link.
    owners = (os.getuid(), 0) if source_binary else (os.getuid(),)
    if (not stat.S_ISREG(info.st_mode) or info.st_uid not in owners
            or (not source_binary and info.st_nlink != 1)
            or (executable and not info.st_mode & 0o111)
            or (private and info.st_mode & 0o077)
            or (no_shared_write and info.st_mode & 0o022)):
        _fail()


def _read(path, *, digest=False, executable=False, private=False,
          no_shared_write=False, source_binary=False):
    parent = _directory(path.parent)
    fd = None
    try:
        before = os.stat(path.name, dir_fd=parent, follow_symlinks=False)
        # Validate before open so a FIFO/device can never block or be consumed.
        _regular(before, executable=executable, private=private,
                 no_shared_write=no_shared_write, source_binary=source_binary)
        fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                     dir_fd=parent)
        info = os.fstat(fd)
        _regular(info, executable=executable, private=private,
                 no_shared_write=no_shared_write, source_binary=source_binary)
        if _identity(before) != _identity(info):
            _fail("artifact_drift")
        result = hashlib.sha256() if digest else bytearray()
        while True:
            chunk = os.read(fd, 65536)
            if not chunk:
                break
            if digest:
                result.update(chunk)
            else:
                result.extend(chunk)
                if len(result) > 1024 * 1024:
                    _fail("artifact_too_large")
        after = os.stat(path.name, dir_fd=parent, follow_symlinks=False)
        if (_identity(info) != _identity(os.fstat(fd))
                or _identity(info) != _identity(after)):
            _fail("artifact_drift")
        check_parent = _directory(path.parent)
        try:
            if _identity(os.fstat(parent))[:2] != _identity(os.fstat(check_parent))[:2]:
                _fail("artifact_drift")
        finally:
            os.close(check_parent)
        return result.hexdigest() if digest else bytes(result)
    finally:
        if fd is not None:
            os.close(fd)
        os.close(parent)


def _guard(function):
    def guarded(*args, **kwargs):
        try:
            return function(*args, **kwargs)
        except PreparationError:
            raise
        except (OSError, ValueError, TypeError, KeyError, UnicodeError, OverflowError,
                RecursionError, NotImplementedError):
            _fail("preparation_error")
    return guarded


def _json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                _fail("invalid_json")
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=pairs,
                      parse_constant=lambda _: _fail("invalid_json"))


@_guard
def create_profile(run_root: Path, port: int, herdr_bin: Path) -> dict:
    if type(port) is not int or not 1 <= port <= 65535:
        _fail("invalid_port")
    _parts(herdr_bin)
    # PATH has no escaping for colons: reject before creating any profile entry.
    if ":" in str(herdr_bin.parent):
        _fail("invalid_path")
    # An explicit canonical source, not a link or executable discovered via PATH.
    _read(herdr_bin, digest=True, executable=True, source_binary=True)
    fd = _directory(run_root)
    try:
        info = os.fstat(fd)
        if (info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != 0o700
                or os.listdir(fd)):
            _fail("invalid_root")
        names = ("home", "home/.codex", "config", "data", "cache", "state",
                 "tmp", "checkout", "config/herdr")
        for name in names:
            parent_name, _, leaf = name.rpartition("/")
            parent = _directory(run_root / parent_name) if parent_name else os.dup(fd)
            try:
                # Ensure owner traversal even under a restrictive caller umask;
                # restore it immediately, including on exclusive-creation failure.
                mask = os.umask(0o077)
                try:
                    os.mkdir(leaf, 0o700, dir_fd=parent)
                finally:
                    os.umask(mask)
                before = os.stat(leaf, dir_fd=parent, follow_symlinks=False)
                child = os.open(leaf, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                dir_fd=parent)
                try:
                    info_child = os.fstat(child)
                    if (not stat.S_ISDIR(info_child.st_mode)
                            or info_child.st_uid != os.getuid()
                            or stat.S_IMODE(info_child.st_mode) != 0o700
                            or _identity(before) != _identity(info_child)):
                        _fail("artifact_drift")
                    os.fchmod(child, 0o700)
                    if _identity(os.fstat(child)) != _identity(os.stat(
                            leaf, dir_fd=parent, follow_symlinks=False)):
                        _fail("artifact_drift")
                finally:
                    os.close(child)
            finally:
                os.close(parent)
        repo = str(run_root / "checkout")
        # JSON's Unicode-preserving basic string syntax matches TOML except DEL.
        trust_key = json.dumps(repo, ensure_ascii=False).replace("\x7f", "\\u007f")
        config = ('model = "gpt-5.4"\nmodel_provider = "local"\n'
                  'check_for_update_on_startup = false\n'
                  'cli_auth_credentials_store = "ephemeral"\n'
                  '[features]\nhooks = true\nshell_snapshot = false\n'
                  f'[projects.{trust_key}]\ntrust_level = "trusted"\n'
                  '[model_providers.local]\nname = "Local diagnostic"\n'
                  f'base_url = "http://127.0.0.1:{port}/v1"\n'
                  'wire_api = "responses"\nrequires_openai_auth = false\n'
                  'supports_websockets = false\n')
        for name, content in (("home/.codex/config.toml", config),
                              ("config/herdr/config.toml", "")):
            path = run_root / name
            parent = _directory(path.parent)
            try:
                out = os.open(path.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL
                              | os.O_NOFOLLOW, 0o600, dir_fd=parent)
                with os.fdopen(out, "wb") as handle:
                    os.fchmod(handle.fileno(), 0o600)
                    handle.write(content.encode("utf-8"))
            finally:
                os.close(parent)
        if _identity(info)[:2] != _identity(os.stat(run_root, follow_symlinks=False))[:2]:
            _fail("artifact_drift")
        env = {key: str(run_root / value) for key, value in {
            "HOME": "home", "CODEX_HOME": "home/.codex", "XDG_CONFIG_HOME": "config",
            "XDG_DATA_HOME": "data", "XDG_CACHE_HOME": "cache",
            "XDG_STATE_HOME": "state", "TMPDIR": "tmp",
            "HERDR_CONFIG_PATH": "config/herdr/config.toml",
            "HERDR_SOCKET_PATH": "config/herdr/s"}.items()}
        env.update(PATH=f"{herdr_bin.parent}:/usr/bin:/bin", LANG="C.UTF-8",
                   TERM="xterm-256color", SHELL="/bin/bash")
        return dict(root=str(run_root), codex_home=env["CODEX_HOME"], repo=repo,
                    socket=env["HERDR_SOCKET_PATH"], env=env)
    finally:
        os.close(fd)


@_guard
def resolve_managed_package(codex_home: Path, start_response: bytes,
                            expected_digest: str) -> dict:
    if (not isinstance(expected_digest, str)
            or not re.fullmatch(r"[0-9a-f]{64}", expected_digest)):
        _fail("invalid_digest")
    if not isinstance(start_response, bytes):
        _fail("invalid_start")
    start = _json(start_response.decode("utf-8"))
    if (not isinstance(start, dict) or start.get("status") != "started"
            or start.get("backend") != "pid" or type(start.get("pid")) is not int
            or start["pid"] <= 0
            or any(start.get(k) != "0.166.0" for k in
                   ("cliVersion", "appServerVersion", "managedCodexVersion"))):
        _fail("invalid_start")
    base = codex_home / "packages/app-server-daemon"
    release = base / "releases/0.166.0-x86_64-unknown-linux-musl"
    if start.get("managedCodexPath") != str(base / "current/bin/codex"):
        _fail("invalid_package_layout")
    control = codex_home / "app-server-control"
    sock = Path(start.get("socketPath", ""))
    _parts(sock)
    if sock != control / "app-server-control.sock":
        _fail("invalid_socket")
    control_fd = _directory(control)
    try:
        info = os.fstat(control_fd)
        if info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != 0o700:
            _fail("invalid_socket")
    finally:
        os.close(control_fd)
    # Only the two captured symlinks are supported; never generic resolve().
    snapshots = []
    for link, target in ((base / "current", str(release)),
                         (release / "codex", "bin/codex")):
        parent = _directory(link.parent)
        try:
            before = os.stat(link.name, dir_fd=parent, follow_symlinks=False)
            if (not stat.S_ISLNK(before.st_mode) or before.st_uid != os.getuid()
                    or os.readlink(link.name, dir_fd=parent) != target):
                _fail("invalid_package_layout")
            snapshots.append((link, _identity(before)))
        finally:
            os.close(parent)
    executable = release / "bin/codex"
    digest = _read(executable, digest=True, executable=True)
    if digest != expected_digest:
        _fail("digest_mismatch")
    for link, identity in snapshots:
        parent = _directory(link.parent)
        try:
            if identity != _identity(os.stat(link.name, dir_fd=parent,
                                            follow_symlinks=False)):
                _fail("artifact_drift")
        finally:
            os.close(parent)
    return dict(executable=str(executable), sha256=digest)


@_guard
def validate_managed_version(version: str) -> None:
    if not isinstance(version, str) or version.strip() != "codex-cli 0.166.0":
        _fail("invalid_version")


@_guard
def validate_hook(codex_home: Path) -> None:
    hook = _json(_read(codex_home / "hooks.json", no_shared_write=True).decode("utf-8"))
    expected = {"hooks": {"SessionStart": [{"hooks": [{
        "command": f"bash '{codex_home / 'herdr-agent-state.sh'}' session",
        "timeout": 10, "type": "command"}]}]}}
    if hook != expected or type(hook["hooks"]["SessionStart"][0]["hooks"][0]["timeout"]) is not int:
        _fail("invalid_hook")
    if _read(codex_home / "herdr-agent-state.sh", digest=True,
             no_shared_write=True) != (
            "c34709efe76f4d83614b47f5b3f5880b621a0f53bf96022393db0d4686b29340"):
        _fail("invalid_hook_digest")


@_guard
def assert_no_auth(codex_home: Path) -> None:
    fd = _directory(codex_home)
    try:
        try:
            os.stat("auth.json", dir_fd=fd, follow_symlinks=False)
        except FileNotFoundError:
            return
        _fail("auth_present")
    finally:
        os.close(fd)


class LocalStub:
    """Bounded, caller-polled HTTP endpoint; never sends data off loopback."""

    def __init__(self, *, clock=time.monotonic):
        self._clock = clock
        self.invalid = False
        self.request_count = 0
        self._accepted = 0
        self._clients = {}
        self._listener = None
        try:
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self._listener = listener
            listener.setblocking(False)
            listener.bind(("127.0.0.1", 0))
            listener.listen(8)
            self.port = listener.getsockname()[1]
        except OSError:
            self.close()
            _fail("stub_socket_error")

    def close(self):
        for client in list(self._clients):
            self._drop(client)
        if self._listener is not None:
            self._listener.close()
            self._listener = None

    def _drop(self, client, invalid=False):
        self.invalid |= invalid
        self._clients.pop(client, None)
        client.close()

    def _response(self, client, state, body, content_type):
        state["out"] = (f"HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n"
                        f"Content-Length: {len(body)}\r\nConnection: close\r\n\r\n"
                        .encode("ascii") + body)
        state["data"].clear()

    def _parse(self, client, state):
        data = state["data"]
        if state["length"] is None:
            end = data.find(b"\r\n\r\n")
            if end < 0:
                if len(data) > 16384:
                    self._drop(client, True)
                return
            if end + 4 > 16384:
                self._drop(client, True)
                return
            lines = bytes(data[:end]).split(b"\r\n")
            request = lines.pop(0).split(b" ")
            if len(request) != 3 or request[2] not in (b"HTTP/1.0", b"HTTP/1.1"):
                self._drop(client, True)
                return
            method, path, _ = request
            headers = {}
            for line in lines:
                key, sep, value = line.partition(b":")
                if (not sep or not re.fullmatch(rb"[!#$%&'*+.^_`|~0-9A-Za-z-]+", key)
                        or any(c < 32 and c != 9 or c == 127 for c in value)):
                    self._drop(client, True)
                    return
                key = key.lower()
                if key in (b"authorization", b"transfer-encoding", b"upgrade") or (
                        key == b"content-length" and key in headers):
                    self._drop(client, True)
                    return
                headers[key] = value.strip(b" \t")
            if any(token.strip() == b"upgrade" for token in
                   headers.get(b"connection", b"").lower().split(b",")):
                self._drop(client, True)
                return
            length = headers.get(b"content-length")
            if length is not None and not re.fullmatch(rb"[0-9]+", length):
                self._drop(client, True)
                return
            significant = (length or b"0").lstrip(b"0") or b"0"
            if len(significant) > 7:
                self._drop(client, True)
                return
            count = int(significant)
            if count > 1048576:
                self._drop(client, True)
                return
            if method == b"GET" and path == b"/v1/models" and count == 0:
                state["kind"] = "models"
            elif method == b"POST" and path == b"/v1/responses" and length is not None:
                state["kind"] = "responses"
            else:
                self._drop(client, True)
                return
            state["length"] = count
            del data[:end + 4]
        if len(data) > state["length"]:
            self._drop(client, True)
            return
        if len(data) < state["length"]:
            return
        if self._accepted >= 64:
            self._drop(client, True)
            return
        if state["kind"] == "models":
            self._accepted += 1
            self._response(client, state, b'{"data":[]}', "application/json")
            return
        try:
            value = _json(bytes(data).decode("utf-8"))
            if not isinstance(value, dict):
                raise ValueError()
        except (ValueError, UnicodeError, PreparationError, RecursionError):
            self._drop(client, True)
            return
        if self.request_count >= 64:
            self._drop(client, True)
            return
        self._accepted += 1
        self.request_count += 1
        number = self.request_count
        events = [
            ("response.created", {"type": "response.created", "response": {"id": f"local-response-{number}"}}),
            ("response.output_item.done", {"type": "response.output_item.done", "item": {
                "type": "message", "role": "assistant", "id": f"msg-{number}",
                "content": [{"type": "output_text", "text": "LOCAL_STUB_OK"}]}}),
            ("response.completed", {"type": "response.completed", "response": {
                "id": f"local-response-{number}", "usage": {
                    "input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}})]
        body = "".join(f"event: {name}\ndata: {json.dumps(value)}\n\n"
                       for name, value in events).encode("utf-8")
        self._response(client, state, body, "text/event-stream")

    def poll(self):
        if self._listener is None:
            return
        try:
            reads = [self._listener] + list(self._clients)
            writes = [c for c, s in self._clients.items() if s["out"]]
            ready, writable, _ = select.select(reads, writes, [], 0)
            if self._listener in ready:
                # One accept per tick also bounds connection-flood work.
                client, _ = self._listener.accept()
                client.setblocking(False)
                if len(self._clients) >= 8:
                    self._drop(client, True)
                else:
                    self._clients[client] = dict(data=bytearray(), length=None,
                                                 out=b"", last_progress=self._clock())
            for client in list(self._clients):
                state = self._clients.get(client)
                try:
                    if client in ready:
                        chunk = client.recv(65536)
                        if state["out"]:
                            if chunk:
                                self._drop(client, True)
                                continue
                        elif not chunk:
                            self._drop(client, True)
                            continue
                        else:
                            state["last_progress"] = self._clock()
                            state["data"].extend(chunk)
                            self._parse(client, state)
                    # Timeout measures inactivity, not total request age. A
                    # late owner may need several bounded ticks to drain queued
                    # bytes; every successful read renews the inactivity window.
                    if (client in self._clients and not state["out"]
                            and self._clock() - state["last_progress"] > 2):
                        self._drop(client, True)
                        continue
                    if client in self._clients and client in writable:
                        sent = client.send(state["out"][:65536])
                        state["out"] = state["out"][sent:]
                        if not state["out"]:
                            self._drop(client)
                except BlockingIOError:
                    pass
                except OSError:
                    self._drop(client, True)
        except (OSError, ValueError):
            # select() can reject descriptors beyond FD_SETSIZE. This is a
            # failed setup, not an exception to leak into the owner's loop.
            self.invalid = True
