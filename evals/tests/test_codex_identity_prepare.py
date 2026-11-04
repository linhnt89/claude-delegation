"""Credential-free, offline preparation tests (no real worker binaries)."""

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import stat
import tempfile
import tomllib
from types import SimpleNamespace
import unittest
from unittest.mock import patch

LIB = Path(__file__).resolve().parents[1] / "lib"
SPEC = importlib.util.spec_from_file_location("prepare", LIB / "codex_identity_prepare.py")
p = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(p)
FIX = LIB / "fixtures/codex-identity"


class Files(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "run"
        self.root.mkdir(mode=0o700)
        self.binary = self.base / "herdr"
        self.binary.write_bytes(b"offline placeholder, never executed")
        self.binary.chmod(0o700)

    def profile(self):
        return p.create_profile(self.root, 40000, self.binary)

    def error(self, function, *args):
        with self.assertRaises(p.PreparationError) as caught:
            function(*args)
        error = caught.exception
        self.assertEqual(str(error), error.code)
        self.assertNotIn(str(self.base), str(error))

    def test_profile_exact_config_and_environment(self):
        with patch.dict(os.environ, {"OPENAI_API_KEY": "secret", "HTTP_PROXY": "secret",
                                     "HERDR_ENV": "1", "CODEX_THREAD_ID": "secret"}):
            result = self.profile()
        self.assertEqual(set(result), {"root", "codex_home", "repo", "socket", "env"})
        env = result["env"]
        self.assertEqual(set(env), {"HOME", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME",
                                   "XDG_CACHE_HOME", "XDG_STATE_HOME", "TMPDIR", "PATH", "LANG",
                                   "TERM", "SHELL", "HERDR_CONFIG_PATH", "HERDR_SOCKET_PATH"})
        self.assertEqual(env["PATH"], f"{self.binary.parent}:/usr/bin:/bin")
        self.assertEqual(env["LANG"], "C.UTF-8")
        self.assertEqual(env["TERM"], "xterm-256color")
        self.assertEqual(env["SHELL"], "/bin/bash")
        config = tomllib.loads((Path(result["codex_home"]) / "config.toml").read_text())
        captured = tomllib.loads((FIX / "config.toml").read_text().replace("/RUN", str(self.root)))
        self.assertEqual(config, captured)
        self.assertEqual(Path(env["HERDR_CONFIG_PATH"]).read_bytes(), b"")
        for entry in self.root.rglob("*"):
            self.assertEqual(stat.S_IMODE(entry.stat().st_mode), 0o700 if entry.is_dir() else 0o600)
        self.error(self.profile)

    def test_restrictive_umask_and_unicode_trust_path(self):
        self.root.rmdir()
        self.root = self.base / 'quote"-𝄞'
        self.root.mkdir(mode=0o700)
        old = os.umask(0o777)
        try:
            result = self.profile()
        finally:
            os.umask(old)
        config = tomllib.loads((Path(result["codex_home"]) / "config.toml").read_text())
        self.assertEqual(config["projects"], {result["repo"]: {"trust_level": "trusted"}})
        for entry in self.root.rglob("*"):
            self.assertEqual(stat.S_IMODE(entry.stat().st_mode), 0o700 if entry.is_dir() else 0o600)

    def test_review3_trust_key_with_del_is_valid_toml_or_rejected(self):
        self.root.rmdir()
        self.root = self.base / "del\x7fname"
        self.root.mkdir(mode=0o700)
        try:
            result = self.profile()
        except p.PreparationError:
            return  # a redacted rejection is acceptable
        config = tomllib.loads((Path(result["codex_home"]) / "config.toml").read_text())
        self.assertEqual(config["projects"], {result["repo"]: {"trust_level": "trusted"}})

    def test_del_and_non_bmp_trust_key_round_trip(self):
        self.root.rmdir()
        self.root = self.base / 'del\x7f-𝄞-"-\\'
        self.root.mkdir(mode=0o700)
        result = self.profile()
        raw = (Path(result["codex_home"]) / "config.toml").read_text()
        self.assertNotIn("\x7f", raw)
        config = tomllib.loads(raw)
        self.assertEqual(config["projects"], {result["repo"]: {"trust_level": "trusted"}})

    def test_new_directories_use_verified_descriptors_not_path_chmod(self):
        original_fchmod = os.fchmod
        directory_calls = []
        def checked_fchmod(fd, mode):
            info = os.fstat(fd)
            if stat.S_ISDIR(info.st_mode):
                self.assertEqual(info.st_uid, os.getuid())
                self.assertEqual(stat.S_IMODE(info.st_mode), 0o700)
                directory_calls.append(fd)
            original_fchmod(fd, mode)
        with patch.object(p.os, "chmod", side_effect=NotImplementedError("private path")), \
                patch.object(p.os, "fchmod", side_effect=checked_fchmod):
            self.profile()
        self.assertEqual(len(directory_calls), 9)

    def test_unsupported_operations_are_redacted_and_descriptors_closed(self):
        original_open = os.open
        descriptors = []
        def tracked_open(*args, **kwargs):
            fd = original_open(*args, **kwargs)
            descriptors.append(fd)
            return fd
        mask = os.umask(0o077)
        os.umask(mask)
        with patch.object(p.os, "open", side_effect=tracked_open), \
                patch.object(p.os, "fchmod", side_effect=NotImplementedError(str(self.base))):
            self.error(self.profile)
        for fd in set(descriptors):
            with self.assertRaises(OSError):
                os.fstat(fd)
        actual = os.umask(mask)
        self.assertEqual(actual, mask)
        (self.root / "home").rmdir()
        with patch.object(p.os, "mkdir", side_effect=NotImplementedError(str(self.base))) as mkdir:
            self.error(self.profile)
        mkdir.assert_called_once()
        actual = os.umask(mask)
        self.assertEqual(actual, mask)

    def test_new_directory_substitution_never_chmods_existing_entry(self):
        other = self.base / "other"
        other.mkdir(mode=0o755)
        original_mkdir = os.mkdir
        def substituted(leaf, mode, *, dir_fd):
            original_mkdir(leaf, mode, dir_fd=dir_fd)
            os.rmdir(leaf, dir_fd=dir_fd)
            os.symlink(other, leaf, dir_fd=dir_fd)
        with patch.object(p.os, "mkdir", side_effect=substituted), \
                patch.object(p.os, "fchmod", side_effect=AssertionError("must not chmod")):
            self.error(self.profile)
        self.assertEqual(stat.S_IMODE(other.stat().st_mode), 0o755)

    def test_import_has_no_resource_side_effects(self):
        with patch.object(p.socket, "socket", side_effect=AssertionError("socket")), \
                patch.object(p.os, "open", side_effect=AssertionError("open")):
            module = importlib.util.module_from_spec(SPEC)
            SPEC.loader.exec_module(module)

    def test_invalid_ports_no_writes(self):
        for port in (True, False, 0, -1, 65536, "40000", None, 1.5):
            self.error(p.create_profile, self.root, port, self.binary)
            self.assertEqual(list(self.root.iterdir()), [])

    def test_root_and_binary_hazards(self):
        self.root.chmod(0o755)
        self.error(self.profile)
        self.root.chmod(0o700)
        alias = self.base / "alias"
        alias.symlink_to(self.root, target_is_directory=True)
        self.error(p.create_profile, alias, 1, self.binary)
        self.error(p.create_profile, Path("relative"), 1, self.binary)
        self.error(p.create_profile, self.root, 1, Path("herdr"))
        binary_alias = self.base / "binary-alias"
        binary_alias.symlink_to(self.binary)
        self.error(p.create_profile, self.root, 1, binary_alias)
        self.binary.chmod(0o600)
        self.error(self.profile)
        self.binary.chmod(0o700)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_source_binary_hardlinks_allowed(self):
        os.link(self.binary, self.base / "hardlink")
        self.profile()
        self.assertEqual(self.binary.stat().st_nlink, 2)

    def test_root_owned_source_binary_with_multiple_links(self):
        os.link(self.binary, self.base / "hardlink")
        identity = self.binary.stat()
        original_stat, original_fstat = os.stat, os.fstat
        def root_owned(info):
            if (info.st_dev, info.st_ino) == (identity.st_dev, identity.st_ino):
                fields = {name: getattr(info, name) for name in dir(info)
                          if name.startswith("st_")}
                fields["st_uid"] = 0
                return SimpleNamespace(**fields)
            return info
        with patch.object(p.os, "stat", side_effect=lambda *a, **kw:
                          root_owned(original_stat(*a, **kw))), \
                patch.object(p.os, "fstat", side_effect=lambda fd:
                             root_owned(original_fstat(fd))):
            self.profile()

    def test_source_binary_type_owner_and_ancestor_guards(self):
        self.binary.unlink()
        os.mkfifo(self.binary, 0o700)
        self.error(self.profile)
        self.binary.unlink()
        self.binary.mkdir(mode=0o700)
        self.error(self.profile)
        self.binary.rmdir()
        self.binary.write_bytes(b"offline placeholder")
        self.binary.chmod(0o700)
        # A different non-root caller must reject this file's actual owner.
        with patch.object(p.os, "getuid", return_value=os.getuid() + 1):
            if self.binary.stat().st_uid != 0:
                self.error(self.profile)
        alias = self.base / "binary-parent-alias"
        alias.symlink_to(self.base, target_is_directory=True)
        self.error(p.create_profile, self.root, 1, alias / "herdr")
        self.assertEqual(list(self.root.iterdir()), [])

    def test_ancestor_symlink(self):
        fd = p._directory(Path("/"))
        try:
            self.assertTrue(stat.S_ISDIR(os.fstat(fd).st_mode))
        finally:
            os.close(fd)
        alias = self.base / "alias"
        alias.symlink_to(self.base, target_is_directory=True)
        self.error(p.create_profile, alias / "run", 1, self.binary)
        self.error(p.assert_no_auth, alias / "run")

    def test_no_auth_never_opens_entry(self):
        p.assert_no_auth(self.root)
        entry = self.root / "auth.json"
        for kind in ("file", "dir", "link", "broken", "fifo"):
            if kind == "file":
                entry.write_text("secret")
            elif kind == "dir":
                entry.mkdir()
            elif kind == "fifo":
                os.mkfifo(entry)
            else:
                entry.symlink_to(self.binary if kind == "link" else self.base / "missing")
            original_open = os.open
            def no_auth_open(path, *args, **kwargs):
                self.assertNotEqual(path, "auth.json")
                return original_open(path, *args, **kwargs)
            with patch.object(p.os, "open", side_effect=no_auth_open):
                self.error(p.assert_no_auth, self.root)
            if kind == "dir":
                entry.rmdir()
            else:
                entry.unlink()

    def hook(self):
        declaration = json.loads((FIX / "local-responses.json").read_text())["hooks"]
        declaration = json.loads(json.dumps(declaration).replace("/RUN/home/.codex", str(self.root)))
        (self.root / "hooks.json").write_text(json.dumps(declaration))
        (self.root / "herdr-agent-state.sh").write_bytes((FIX / "herdr-agent-state.sh").read_bytes())
        return declaration

    def test_hook_success_and_structural_failures(self):
        original = self.hook()
        p.validate_hook(self.root)
        mutations = []
        for key in ("OtherEvent", "SessionEnd"):
            value = copy.deepcopy(original)
            value["hooks"][key] = []
            mutations.append(value)
        value = copy.deepcopy(original)
        value["extra"] = True
        mutations.append(value)
        for key, val in (("timeout", 10.0), ("timeout", True), ("type", "shell"),
                         ("command", "bash wrong session"), ("extra", True)):
            value = copy.deepcopy(original)
            value["hooks"]["SessionStart"][0]["hooks"][0][key] = val
            mutations.append(value)
        value = copy.deepcopy(original)
        value["hooks"]["SessionStart"] *= 2
        mutations.append(value)
        mutations.extend(({}, [], None))
        for value in mutations:
            (self.root / "hooks.json").write_text(json.dumps(value))
            self.error(p.validate_hook, self.root)
        self.hook()
        (self.root / "herdr-agent-state.sh").write_text("modified")
        self.error(p.validate_hook, self.root)

    def test_hook_modes_allow_installed_bits_but_not_shared_writes(self):
        self.hook()
        hooks = self.root / "hooks.json"
        script = self.root / "herdr-agent-state.sh"
        hooks.chmod(0o644)
        script.chmod(0o755)
        p.validate_hook(self.root)
        for path, safe_mode, unsafe_modes in (
                (hooks, 0o644, (0o666, 0o664, 0o646)),
                (script, 0o755, (0o777, 0o775, 0o757))):
            for mode in unsafe_modes:
                with self.subTest(file=path.name, mode=oct(mode)):
                    path.chmod(mode)
                    before = path.read_bytes()
                    self.error(p.validate_hook, self.root)
                    self.assertEqual(stat.S_IMODE(path.stat().st_mode), mode)
                    self.assertEqual(path.read_bytes(), before)
                    path.chmod(safe_mode)
        p.validate_hook(self.root)

    def test_hook_files_links_type_owner_and_drift(self):
        self.hook()
        script = self.root / "herdr-agent-state.sh"
        other = self.base / "other"
        os.link(script, other)
        self.error(p.validate_hook, self.root)
        other.unlink()
        script.unlink()
        script.symlink_to(FIX / "herdr-agent-state.sh")
        self.error(p.validate_hook, self.root)
        script.unlink()
        os.mkfifo(script)
        self.error(p.validate_hook, self.root)
        script.unlink()
        self.hook()
        original_read = os.read
        changed = False
        def drifting(fd, count):
            nonlocal changed
            chunk = original_read(fd, count)
            if not changed:
                changed = True
                with (self.root / "hooks.json").open("ab") as out:
                    out.write(b" ")
            return chunk
        with patch.object(p.os, "read", side_effect=drifting):
            self.error(p.validate_hook, self.root)
        with patch.object(p.os, "getuid", return_value=os.getuid() + 1):
            self.error(p.validate_hook, self.root)

    def package(self):
        home = self.root
        fixture = json.loads((FIX / "provisioning.json").read_text())
        start = json.loads(json.dumps(fixture["start_stdout"]).replace("/RUN/home/.codex", str(home)))
        release = home / "packages/app-server-daemon/releases/0.166.0-x86_64-unknown-linux-musl"
        (release / "bin").mkdir(parents=True)
        (release / "bin/codex").write_bytes(b"pinned offline artifact")
        (release / "bin/codex").chmod(0o700)
        (release / "codex").symlink_to("bin/codex")
        (release.parent.parent / "current").symlink_to(release, target_is_directory=True)
        (home / "app-server-control").mkdir(mode=0o700)
        digest = hashlib.sha256(b"pinned offline artifact").hexdigest()
        return start, release, digest

    def test_package_success_and_invalid_metadata(self):
        start, release, digest = self.package()
        def resolve(value=start, expected=digest):
            return p.resolve_managed_package(self.root, json.dumps(value).encode(), expected)
        self.assertEqual(resolve(), {"executable": str(release / "bin/codex"), "sha256": digest})
        for field, values in {"status": ["running", None], "backend": ["other"],
                              "pid": [True, 0, -1, "4001"],
                              "cliVersion": ["0.165.0"], "managedCodexVersion": ["0.167.0"],
                              "appServerVersion": [None], "managedCodexPath": ["/bin/sh"],
                              "socketPath": ["/tmp/s", str(self.root / "app-server-control/../s")]}.items():
            for value in values:
                bad = dict(start, **{field: value})
                self.error(resolve, bad)
        for expected in ("0" * 64, "bad", "A" * 64, None):
            self.error(resolve, start, expected)
        for data in (b"{}", b"[]", b"null", b"bad", b"\xff", b'{"pid":1,"pid":2}'):
            self.error(p.resolve_managed_package, self.root, data, digest)

    def test_package_link_and_file_hazards(self):
        start, release, digest = self.package()
        def resolve():
            return p.resolve_managed_package(self.root, json.dumps(start).encode(), digest)
        current = release.parent.parent / "current"
        current.unlink()
        current.symlink_to(self.base)
        self.error(resolve)
        current.unlink()
        current.symlink_to(release)
        alias = release / "codex"
        alias.unlink()
        alias.symlink_to("../../../../escape")
        self.error(resolve)
        alias.unlink()
        alias.symlink_to("bin/codex")
        exe = release / "bin/codex"
        os.link(exe, self.base / "hard")
        self.error(resolve)
        (self.base / "hard").unlink()
        exe.chmod(0o600)
        self.error(resolve)
        exe.chmod(0o700)
        exe.unlink()
        exe.symlink_to(self.binary)
        self.error(resolve)

    def test_package_drift_and_private_control(self):
        start, release, digest = self.package()
        response = json.dumps(start).encode()
        control = self.root / "app-server-control"
        control.chmod(0o755)
        self.error(p.resolve_managed_package, self.root, response, digest)
        control.chmod(0o700)
        original_read = os.read
        current = release.parent.parent / "current"
        def drifting(fd, count):
            data = original_read(fd, count)
            if current.is_symlink():
                current.unlink()
                current.mkdir()
            return data
        with patch.object(p.os, "read", side_effect=drifting):
            self.error(p.resolve_managed_package, self.root, response, digest)

    def test_version_is_exact_stripped(self):
        for value in ("codex-cli 0.166.0", " codex-cli 0.166.0\n"):
            p.validate_managed_version(value)
        for value in (None, b"codex-cli 0.166.0", "codex-cli 0.166.1", "0.166.0",
                      "codex-cli  0.166.0", "codex-cli 0.166.0\nextra"):
            self.error(p.validate_managed_version, value)


class Stub(unittest.TestCase):
    def setUp(self):
        self.now = 10.0
        self.stub = p.LocalStub(clock=lambda: self.now)
        self.addCleanup(self.stub.close)

    def client(self):
        client = socket.create_connection(("127.0.0.1", self.stub.port), timeout=1)
        client.setblocking(False)
        self.addCleanup(client.close)
        self.stub.poll()
        return client

    def exchange(self, request):
        client = self.client()
        client.sendall(request)
        response = bytearray()
        for _ in range(100):
            self.stub.poll()
            try:
                chunk = client.recv(65536)
            except BlockingIOError:
                continue
            except ConnectionResetError:
                break
            if not chunk:
                break
            response.extend(chunk)
        return bytes(response)

    @staticmethod
    def post(body=b"{}", headers=b""):
        return (b"POST /v1/responses HTTP/1.1\r\nContent-Length: " + str(len(body)).encode()
                + b"\r\n" + headers + b"\r\n" + body)

    def test_exact_sse_repeated_and_auxiliary(self):
        fixture = json.loads((FIX / "local-responses.json").read_text())
        for number in (1, 2):
            response = self.exchange(self.post())
            head, body = response.split(b"\r\n\r\n", 1)
            expected = fixture["sse"].replace("local-response-1", f"local-response-{number}").replace("msg-1", f"msg-{number}")
            self.assertEqual(body.decode(), expected)
            self.assertIn(b"Content-Type: text/event-stream", head)
            self.assertIn(f"Content-Length: {len(body)}".encode(), head)
        response = self.exchange(b"GET /v1/models HTTP/1.0\r\n\r\n")
        self.assertEqual(json.loads(response.split(b"\r\n\r\n")[1]), {"data": []})
        self.assertEqual(self.stub.request_count, 2)
        self.assertFalse(self.stub.invalid)

    def test_authorization_any_casing_empty_and_duplicate(self):
        for headers in (b"Authorization: secret\r\n", b"aUtHoRiZaTiOn:\r\n",
                        b"authorization:\r\nauthorization:\r\n"):
            self.assertEqual(self.exchange(self.post(headers=headers)), b"")
            self.assertTrue(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 0)

    def test_malformed_http(self):
        requests = [b"garbage\r\n\r\n", b"POST /v1/responses HTTP/2\r\n\r\n",
                    b"GET /other HTTP/1.1\r\n\r\n", b"POST /v1/responses HTTP/1.1\r\n\r\n",
                    b"POST /v1/responses HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
                    b"POST /v1/responses HTTP/1.1\r\nContent-Length: 1048577\r\n\r\n",
                    b"POST /v1/responses HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
                    b"GET /v1/models HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
                    b"GET /v1/models HTTP/1.1\r\nUpgrade: websocket\r\n\r\n",
                    b"GET /v1/models HTTP/1.1\r\nContent-Length: 1\r\n\r\nx",
                    b"GET /v1/models HTTP/1.1\r\nbad header\r\n\r\n",
                    self.post() + self.post(), self.post(b"[]"), self.post(b"null"),
                    self.post(b"\xff"), self.post(b"NaN"), self.post(b"{"),
                    b"G" * 16385, b"GET /v1/models HTTP/1.1\r\nX: " + b"a" * 16384 + b"\r\n\r\n"]
        for request in requests:
            with self.subTest(request=request[:50]):
                self.assertEqual(self.exchange(request), b"")
                self.assertTrue(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 0)

    def test_partial_header_and_body_deadline(self):
        for fragment in (b"POST /v1/responses HTTP/1.1\r\n", self.post()[:-1]):
            client = self.client()
            client.sendall(fragment)
            self.stub.poll()
            self.now += 2.01
            self.stub.poll()
            self.assertTrue(self.stub.invalid)
            self.assertEqual(client.recv(100), b"")
        self.assertEqual(self.stub.request_count, 0)

    def test_content_length_leading_zeroes(self):
        response = self.exchange(b"POST /v1/responses HTTP/1.1\r\nContent-Length: 0000000002\r\n\r\n{}")
        self.assertTrue(response.startswith(b"HTTP/1.1 200"))
        self.assertFalse(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 1)

    def test_fragmented_body_and_header_success(self):
        client = self.client()
        for byte in self.post():
            client.send(bytes([byte]))
            self.stub.poll()
        for _ in range(5):
            self.stub.poll()
        self.assertEqual(self.stub.request_count, 1)
        self.assertFalse(self.stub.invalid)

    def test_request_limit(self):
        for _ in range(64):
            self.assertTrue(self.exchange(self.post()).startswith(b"HTTP/1.1 200"))
        self.assertEqual(self.exchange(self.post()), b"")
        self.assertTrue(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 64)

    def test_auxiliary_limit_and_authorization(self):
        request = b"GET /v1/models HTTP/1.1\r\n\r\n"
        for _ in range(64):
            self.assertTrue(self.exchange(request))
        self.assertEqual(self.stub.request_count, 0)
        self.assertEqual(self.exchange(request), b"")
        self.assertTrue(self.stub.invalid)
        self.assertEqual(self.exchange(b"GET /v1/models HTTP/1.1\r\nAuthorization:\r\n\r\n"), b"")

    def test_large_body_is_bounded_and_accepted(self):
        body = json.dumps({"input": "x" * 900000}).encode()
        client = self.client()
        wire = self.post(body)
        # Small sends interleaved with polling avoid socket-buffer assumptions.
        for offset in range(0, len(wire), 8192):
            client.sendall(wire[offset:offset + 8192])
            self.stub.poll()
        for _ in range(20):
            self.stub.poll()
        self.assertEqual(self.stub.request_count, 1)
        self.assertFalse(self.stub.invalid)

    def test_socket_errors_are_redacted_and_teardown_idempotent(self):
        with patch.object(p.socket, "socket", side_effect=OSError("secret path")):
            with self.assertRaises(p.PreparationError) as caught:
                p.LocalStub()
        self.assertEqual(str(caught.exception), "stub_socket_error")
        self.client()
        with patch.object(p.select, "select", side_effect=OSError("secret body")):
            self.stub.poll()
        self.assertTrue(self.stub.invalid)
        self.stub.close()
        self.stub.close()

    def test_connection_limit_and_close(self):
        clients = [self.client() for _ in range(9)]
        self.assertTrue(self.stub.invalid)
        self.assertLessEqual(len(self.stub._clients), 8)
        fds = [c.fileno() for c in self.stub._clients] + [self.stub._listener.fileno()]
        self.stub.close()
        self.stub.close()
        self.stub.poll()
        self.assertTrue(self.stub.invalid)
        for fd in fds:
            with self.assertRaises(OSError):
                os.fstat(fd)
        for client in clients:
            client.close()

    def test_early_eof_invalid(self):
        client = self.client()
        client.sendall(b"POST")
        client.shutdown(socket.SHUT_WR)
        self.stub.poll()
        self.stub.poll()
        self.assertTrue(self.stub.invalid)


    def test_review_complete_request_survives_slow_owner_tick(self):
        # Review evidence: the request is complete in the socket before the
        # deadline; only the owner's poll tick is late (e.g. a setup command).
        client = self.client()
        client.sendall(self.post())
        self.now += 2.5
        for _ in range(5):
            self.stub.poll()
        self.assertFalse(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 1)

    def test_ready_but_incomplete_request_expires_after_read_inactivity(self):
        client = self.client()
        client.sendall(self.post()[:-1])
        self.now += 2.5
        self.stub.poll()
        self.assertFalse(self.stub.invalid)
        self.now += 2.5
        self.stub.poll()
        self.assertTrue(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 0)
        self.assertEqual(client.recv(100), b"")

    def test_ready_body_completes_after_slow_owner_tick(self):
        client = self.client()
        wire = self.post()
        client.sendall(wire[:-1])
        self.stub.poll()
        client.sendall(wire[-1:])
        self.now += 2.5
        self.stub.poll()
        self.assertFalse(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 1)

    def test_review2_complete_large_request_survives_slow_owner_tick(self):
        # Review round 3 evidence: a valid (<=1MiB) request fully buffered in the
        # socket before the deadline is expired after one 64KiB read on a late tick.
        client = self.client()
        body = json.dumps({"input": "x" * 200000}).encode()
        client.sendall(self.post(body))
        self.now += 2.5
        self.stub.poll()
        self.assertFalse(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 0)  # Only one <=64KiB read.
        for _ in range(9):
            self.now += 2.5  # Every progress tick survives even a late owner.
            self.stub.poll()
        self.assertFalse(self.stub.invalid)
        self.assertEqual(self.stub.request_count, 1)

    def test_review_high_descriptor_poll_is_redacted_error(self):
        # Review evidence: select() rejects descriptors >= FD_SETSIZE with a raw
        # ValueError; poll must set invalid instead of escaping to the owner loop.
        import resource
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        if hard != resource.RLIM_INFINITY and hard < 2048:
            self.skipTest("cannot raise descriptor limit")
        resource.setrlimit(resource.RLIMIT_NOFILE, (2048, hard))
        self.addCleanup(resource.setrlimit, resource.RLIMIT_NOFILE, (soft, hard))
        held = [os.open("/dev/null", os.O_RDONLY) for _ in range(1100)]
        try:
            stub = p.LocalStub(clock=lambda: self.now)
            try:
                stub.poll()
                self.assertTrue(stub.invalid)
            finally:
                stub.close()
        finally:
            for fd in held:
                os.close(fd)

class ReviewRound4(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "run"
        self.root.mkdir(mode=0o700)

    def test_review4_herdr_parent_with_colon_cannot_split_pinned_path(self):
        # A ':' in the Herdr parent splits PATH into a truncated directory and a
        # RELATIVE entry ("in"), so the PATH no longer pins only the Herdr parent.
        parent = self.base / "b:in"
        parent.mkdir()
        binary = parent / "herdr"
        binary.write_bytes(b"offline placeholder, never executed")
        binary.chmod(0o700)
        try:
            result = p.create_profile(self.root, 40000, binary)
        except p.PreparationError as error:
            self.assertEqual(str(error), error.code)
            self.assertEqual(list(self.root.iterdir()), [])
            return
        entries = result["env"]["PATH"].split(":")
        self.assertEqual(entries, [str(parent), "/usr/bin", "/bin"])

    def test_review4_root_owned_system_herdr_is_accepted(self):
        # Spec: herdr_bin is an explicit absolute canonical regular executable;
        # it does not require caller ownership (e.g. a packaged /usr/bin/herdr).
        candidate = Path("/usr/bin/true")
        if (os.getuid() == 0 or not candidate.is_file() or candidate.is_symlink()
                or os.path.realpath(candidate) != str(candidate)
                or candidate.stat().st_uid != 0
                or candidate.stat().st_nlink != 1):
            self.skipTest("no canonical root-owned single-link executable")
        p.create_profile(self.root, 40000, candidate)


if __name__ == "__main__":
    unittest.main()
