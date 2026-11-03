#!/usr/bin/env python3
"""Implementation smoke tests; not independent P030–P033 verdicts."""
import contextlib
import copy
import json
import io
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = Path(__file__).resolve().parent
OBSERVER = HERE / "observer"
BINARY = os.environ["OBSERVER_TEST_DELEGATE"]


class SetupFailure(Exception):
    """Explicit private fixture/tool preparation failed, not a product verdict."""


@contextlib.contextmanager
def provisioning(operation):
    """Only wrap explicit private fixture operations, never product calls."""
    try:
        yield
    except (OSError, subprocess.SubprocessError) as exc:
        raise SetupFailure(operation) from exc


class SmokeResult(unittest.TextTestResult):
    def addError(self, test, err):
        if issubclass(err[0], SetupFailure):
            self.setup_errors = getattr(self, "setup_errors", 0) + 1
        super().addError(test, err)

    def exit_code(self):
        setup = getattr(self, "setup_errors", 0)
        if self.failures or len(self.errors) > setup:
            return 1
        return 3 if setup else 0


class FixtureAPI:
    def __init__(self, path):
        self.path = path
        self.panes = {p["pane_id"]: p for p in json.loads((HERE / "fixtures/panes.json").read_text())["panes"]}
        self.requests = []
        self.errors = []
        self.mode = "normal"
        self.stop = threading.Event()
        with provisioning("private fixture socket creation failed"):
            self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            with provisioning("private fixture socket setup failed"):
                self.listener.bind(str(path))
                self.listener.listen()
                self.listener.settimeout(0.1)
        except SetupFailure:
            self.listener.close()
            raise
        self.thread = threading.Thread(target=self.serve)
        self.thread.start()

    def serve(self):
        while not self.stop.is_set():
            try:
                client, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            with client:
                try:
                    client.settimeout(3)
                    raw = bytearray()
                    while b"\n" not in raw:
                        data = client.recv(4096)
                        if not data or len(raw) > 16384:
                            raise ValueError("missing/bad newline request")
                        raw.extend(data)
                    if not raw.endswith(b"\n"):
                        raise ValueError("non-newline framing")
                    request = json.loads(raw)
                    self.requests.append(request)
                    if self.mode == "disconnect":
                        continue
                    if request["method"] == "pane.report_agent_session":
                        params = request["params"]
                        self.panes[params["pane_id"]]["agent_session"] = {
                            "source": params["source"], "agent": params["agent"],
                            "kind": "id", "value": params["agent_session_id"],
                        }
                        client.sendall((json.dumps({"id": request["id"], "result": {"type": "ok"}}) + "\n").encode())
                        continue
                    if request["method"] != "pane.get":
                        raise ValueError("unexpected API method: " + request["method"])
                    if set(request) != {"id", "method", "params"} or set(request["params"]) != {"pane_id"}:
                        raise ValueError("unexpected request shape")
                    pane = self.panes[request["params"]["pane_id"]]
                    reply = {"id": request["id"], "result": {"type": "pane_info", "pane": pane}}
                    client.sendall((json.dumps(reply) + "\n").encode())
                except (OSError, KeyError, ValueError) as exc:
                    self.errors.append(str(exc))

    def close(self):
        self.stop.set()
        self.thread.join(timeout=4)
        self.listener.close()
        self.path.unlink(missing_ok=True)


def validate_fixture(pane, contract):
    def validate(value, fields):
        for key, rule in fields.items():
            if key not in value:
                assert not rule["required"], key
                continue
            if value[key] is None:
                assert rule["nullable"], key
            elif rule["type"] == "string":
                assert isinstance(value[key], str), key
                if "enum" in rule:
                    assert value[key] in rule["enum"], key
            else:
                assert isinstance(value[key], dict), key
                validate(value[key], contract["AgentSessionInfo_subset"])
    validate(pane, contract["PaneInfo_subset"])


class Smoke(unittest.TestCase):
    def setUp(self):
        # Product init/register and their result validation are product verdicts,
        # even when exercised during setUp. Only explicit tool/provisioning
        # failures may raise SetupFailure; never relabel arbitrary exceptions.
        self.setup_private()

    def setup_private(self):
        scratch = HERE / ".scratch"
        scratch.mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="smoke-", dir=scratch)
        self.root = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        self.state = self.root / "experiment"
        # A short explicit /proc directory-fd alias avoids AF_UNIX's path limit
        # for deeply nested worktrees, without /tmp masking or sandbox changes.
        directory_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
        self.addCleanup(os.close, directory_fd)
        self.socket = Path(f"/proc/{os.getpid()}/fd/{directory_fd}/fixture.sock")
        self.api = FixtureAPI(self.socket)
        self.addCleanup(self.api.close)
        self.controls = self.root / "controls.json"
        self.controls.write_text("{}")
        self.calls = self.root / "delegate-calls.jsonl"
        self.launcher = self.root / "delegate-launcher"
        self.launcher.write_text('''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
with Path(CALLS).open("a") as f: f.write(json.dumps(sys.argv[1:]) + "\\n")
c = json.loads(Path(CONTROLS).read_text())
args = sys.argv[1:]
if args and args[0] == "status":
    key = "all" if args == ["status", "--json"] else args[1]
    mode = c.get(key)
    if mode == "fail": sys.exit(1)
    if mode == "oversized": print("x" * (3 * 1024 * 1024)); sys.exit(0)
    if mode == "malformed": print("not-json"); sys.exit(0)
    if isinstance(mode, dict): print(json.dumps(mode)); sys.exit(0)
if args and args[0] == "discard" and c.get("discard_fail"): sys.exit(1)
if args and args[0] == "new" and c.get("new_fail"): sys.exit(1)
if args and args[0] == "new" and c.get("new_race"):
    import subprocess
    subprocess.run([BINARY] + args, check=True, stdout=subprocess.DEVNULL)
    sys.exit(1)
os.execv(BINARY, [BINARY] + args)
'''.replace("CALLS", repr(str(self.calls))).replace("CONTROLS", repr(str(self.controls))).replace("BINARY", repr(BINARY)))
        self.launcher.chmod(0o700)
        self.cli("init", "--delegate", str(self.launcher), "--socket", str(self.socket))
        self.repo = self.make_repo("checkout")
        self.namespace = self.cli("register", "--checkout", str(self.repo))["checkout_namespace"]

    def env(self):
        # Test fixture preparation also uses explicit isolation, not ambient git/XDG.
        home = self.state / "home"
        return {"PATH": "/usr/bin:/bin", "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache"),
                "XDG_STATE_HOME": str(home / ".local/state"), "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": str(home / ".gitconfig"), "GIT_TERMINAL_PROMPT": "0",
                "GIT_AUTHOR_NAME": "Synthetic", "GIT_AUTHOR_EMAIL": "test@example.test",
                "GIT_COMMITTER_NAME": "Synthetic", "GIT_COMMITTER_EMAIL": "test@example.test"}

    def command(self, argv, cwd=None, env=None):
        try:
            return subprocess.run([str(a) for a in argv], cwd=cwd, env=env or self.env(),
                                  stdin=subprocess.DEVNULL, capture_output=True, timeout=90, check=True)
        except FileNotFoundError as exc:
            raise SetupFailure("explicit test tool missing") from exc

    def make_repo(self, name):
        repo = self.root / name
        repo.mkdir()
        with provisioning("private git fixture init failed"):
            self.command(["git", "init", "-q"], repo)
        (repo / "README.md").write_text("Disposable synthetic checkout\n")
        with provisioning("private git fixture add/commit failed"):
            self.command(["git", "add", "."], repo)
            self.command(["git", "commit", "-qm", "Initial"], repo)
        return repo

    def cli(self, *args, code=0, env=None):
        try:
            result = subprocess.run([str(OBSERVER), "--state", str(self.state), *args], env=env,
                                    capture_output=True, timeout=90)
        except FileNotFoundError as exc:
            raise SetupFailure("prototype executable missing") from exc
        self.assertEqual(result.returncode, code, (args, result.stdout[:500], result.stderr[:500]))
        return json.loads(result.stdout) if code == 0 else result

    def create(self, task="t-one", pane="pane-a", repo=None, **kwargs):
        return self.cli("create", "--checkout", str(repo or self.repo), "--id", task, "--pane", pane, **kwargs)

    def discard(self, task="t-one", repo=None):
        return self.cli("discard", "--checkout", str(repo or self.repo), "--id", task)

    def snap(self):
        return self.cli("snapshot")

    def rows(self):
        return {(s["checkout_namespace"], t["task_id"]): t for s in self.snap()["sources"] for t in s["tasks"]}

    def row(self, task="t-one", namespace=None):
        return self.rows()[(namespace or self.namespace, task)]

    def taskdir(self, task="t-one", namespace=None):
        matches = list((self.state / "home/.local/share/delegate/checkouts").glob((namespace or self.namespace) + "-*"))
        self.assertEqual(len(matches), 1)
        return matches[0] / task

    def control(self, value):
        self.controls.write_text(json.dumps(value))

    def db(self):
        return json.loads((self.state / "state.json").read_text())

    def run_worker(self, task="t-one", env=None):
        return self.cli("run", "--checkout", str(self.repo), "--id", task, env=env)

    def write_events(self, task, targets):
        events = self.taskdir(task) / "events.jsonl"
        events.write_text("".join(json.dumps({"id": i, "at": "2026-01-01T00:00:00Z", "kind": "read", "target": target, "result": None}) + "\n" for i, target in enumerate(targets, 1)))

    def test_fixture_contract_and_explicit_setup(self):
        contract = json.loads((HERE / "fixtures/contract.json").read_text())
        self.assertEqual(contract["method"], "pane.get")
        self.assertEqual(contract["upstream_commit"], "2ecf19a92fc3a7fff75b6ce3655ef04f4625ec01")
        for pane in self.api.panes.values():
            validate_fixture(pane, contract)
        bad = {"pane_id": "bad", "agent_session": {"source": "x"}}
        with self.assertRaises(AssertionError):
            validate_fixture(bad, contract)
        help_result = self.command([OBSERVER, "--help"])
        self.assertIn(b"snapshot", help_result.stdout)
        result = subprocess.run([str(OBSERVER), "--state", str(self.root / "missing"), "snapshot"], capture_output=True)
        self.assertEqual(result.returncode, 3)
        self.assertFalse((self.root / "missing").exists())
        unregistered = self.make_repo("unregistered")
        self.create(repo=unregistered, code=3)
        self.assertFalse((unregistered / ".git/delegate-checkout-id").exists())
        self.cli("capture", "--pane", "pane-pi")
        self.assertEqual(self.api.requests[-1]["method"], "pane.get")
        self.assertEqual(self.api.errors, [])

    def test_source_qualified_origins_unknown_and_pane_reuse(self):
        a = self.create("t-a")
        b = self.create("t-b", "pane-b")
        self.assertNotEqual(a["origin"], b["origin"])
        self.assertEqual(a["origin"]["value"], b["origin"]["value"])
        self.create("t-unknown", "pane-unknown")
        # Legacy ownership is deliberately conflicting; adapter never falls back.
        self.taskdir("t-unknown").joinpath("STATE.json").write_text(json.dumps({"owner": {"session": "conflicting-synthetic-legacy", "dir": str(self.repo)}, "base": None, "rounds": [], "review": None}))
        self.assertIsNone(self.row("t-unknown")["origin"])
        self.api.panes["pane-a"]["agent_session"] = copy.deepcopy(b["origin"])
        self.create("t-replacement")
        self.assertEqual(self.row("t-a")["origin"], a["origin"])
        self.assertEqual(self.row("t-replacement")["origin"], b["origin"])
        self.api.panes["pane-b"]["agent_session"] = copy.deepcopy(a["origin"])
        self.create("t-resumed", "pane-b")
        self.assertEqual(self.row("t-resumed")["origin"], a["origin"])
        # Same reference value but a different kind remains a different identity.
        self.api.panes["pane-pi"]["agent_session"]["value"] = "/synthetic/ref"
        path_origin = self.create("t-path", "pane-pi")["origin"]
        self.api.panes["pane-pi"]["agent_session"]["kind"] = "id"
        id_origin = self.create("t-id", "pane-pi")["origin"]
        self.assertNotEqual(path_origin, id_origin)
        for i, ref in enumerate(({"source": "x", "agent": "pi", "kind": "future", "value": "synthetic"}, {"source": "x", "agent": "pi", "kind": "id"}, None)):
            self.api.panes["pane-null"]["agent_session"] = ref
            self.assertIsNone(self.create("t-bad-" + str(i), "pane-null")["origin"])
        self.assertEqual(self.api.errors, [])

    def test_scrubbed_run_retry_and_later_capture(self):
        original = self.create()
        # Deliberately inject a simulated leak *inside* the private stand-in launcher,
        # proving that the same wrapper run/bwrap can actually reach and mutate the
        # fixture. No sandbox flags/permissions change and no personal endpoint exists.
        stub = self.state / "bin/pi"
        normal = stub.read_text()
        worker_socket = "/proc/self/cwd/" + os.path.relpath(self.root / "fixture.sock", self.taskdir() / "worktree")
        leak = ("os.environ['HERDR_SOCKET_PATH'] = " + repr(worker_socket) + "\n"
                "os.environ['HERDR_PANE_ID'] = 'pane-a'\n")
        stub.write_text(normal.replace('home = Path(os.environ["HOME"])', leak + 'home = Path(os.environ["HOME"])'))
        self.run_worker()
        self.assertEqual(self.cli("capture", "--pane", "pane-a")["origin"]["value"], "worker-must-not-report")
        self.assertTrue(any(r["method"] == "pane.report_agent_session" for r in self.api.requests))
        self.api.panes["pane-a"]["agent_session"] = copy.deepcopy(original["origin"])
        stub.write_text(normal)
        for record in (self.state / "home/.cache/observer-worker").glob("*.json"):
            record.unlink()
        self.api.requests.clear()
        inherited = dict(os.environ, HERDR_SOCKET_PATH=worker_socket, HERDR_PANE_ID="pane-a", HERDR_TAB_ID="synthetic-tab", HERDR_WORKSPACE_ID="synthetic-workspace", CLAUDE_CODE_SESSION_ID="worker-must-not-own", PI_SESSION_ID="worker-must-not-own", CODEX_THREAD_ID="worker-must-not-own", DELEGATE_HOME="/must-not-use", DELEGATE_SANDBOX="0", DELEGATE_SANDBOXED="1", DELEGATE_WORKERS="/must-not-use", EVALS_HIDDEN="/must-not-use", EVALS_RESULTS="/must-not-use", XDG_DATA_HOME="/must-not-use")
        self.run_worker(env=inherited)
        first = self.row()
        self.assertEqual(first["instance_token"], original["instance_token"])
        self.assertEqual(first["origin"], original["origin"])
        self.assertIsNone(first["attempt_id"])
        self.assertTrue(first["steps"])
        self.run_worker(env=inherited)
        second = self.row()
        self.assertEqual(second["instance_token"], original["instance_token"])
        self.assertEqual(len(second["steps"]), len(first["steps"]))
        records = list((self.state / "home/.cache/observer-worker").glob("*.json"))
        self.assertEqual(len(records), 2)
        for record in records:
            value = json.loads(record.read_text())
            self.assertEqual(value["hints"], {})
            self.assertTrue(value["stdin_eof"])
            # delegate itself deliberately adds task-home/sandbox markers inside
            # bwrap; these must be isolated values, not the inherited overrides.
            self.assertEqual(value["delegate_home"], str(self.taskdir().parent))
            self.assertEqual(value["sandboxed"], "1")
            self.assertNotIn("EVALS_HIDDEN", value["environment_keys"])
        after = self.create("t-later")["origin"]
        self.assertEqual(after, original["origin"])
        self.assertEqual(self.api.errors, [])
        self.assertTrue(all(r["method"] == "pane.get" for r in self.api.requests))

    def test_linked_worktree_duplicate_and_distinct_namespace(self):
        origin = self.create()
        linked = self.root / "linked"
        self.command(["git", "worktree", "add", "-qb", "linked-lead", linked], self.repo)
        linked_namespace = self.cli("register", "--checkout", str(linked))["checkout_namespace"]
        self.assertEqual(linked_namespace, self.namespace)
        before = self.db()["sources"][self.namespace]["slots"]
        requests = len(self.api.requests)
        self.create(repo=linked, pane="pane-b", code=3)
        self.assertEqual(len(self.api.requests), requests)
        self.assertEqual(self.db()["sources"][self.namespace]["slots"], before)
        self.assertEqual(self.row()["instance_token"], origin["instance_token"])
        self.control({"all": "fail"})
        self.create(repo=linked, code=3)
        self.assertEqual(self.db()["sources"][self.namespace]["slots"], before)
        self.control({})
        other = self.make_repo("other-checkout")
        namespace = self.cli("register", "--checkout", str(other))["checkout_namespace"]
        self.assertNotEqual(namespace, self.namespace)
        other_origin = self.create(repo=other, pane="pane-b")
        rows = self.rows()
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[(namespace, "t-one")]["origin"], other_origin["origin"])
        self.assertNotEqual(rows[(namespace, "t-one")]["instance_token"], origin["instance_token"])
        self.assertEqual(len(self.snap()["sources"]), 2)

    def test_concurrent_linked_setup_and_serial_snapshot_generations(self):
        linked = self.root / "linked"
        self.command(["git", "worktree", "add", "-qb", "linked-lead", linked], self.repo)
        self.cli("register", "--checkout", str(linked))
        procs = [subprocess.Popen([str(OBSERVER), "--state", str(self.state), "create", "--checkout", str(repo), "--id", "t-race", "--pane", pane], stdout=subprocess.PIPE, stderr=subprocess.PIPE) for repo, pane in ((self.repo, "pane-a"), (linked, "pane-b"))]
        results = [p.communicate(timeout=30) for p in procs]
        self.assertEqual(sorted(p.returncode for p in procs), [0, 3])
        winner = next(json.loads(out) for p, (out, _) in zip(procs, results) if p.returncode == 0)
        self.assertEqual(self.row("t-race")["instance_token"], winner["instance_token"])
        reads = [subprocess.Popen([str(OBSERVER), "--state", str(self.state), "snapshot"], stdout=subprocess.PIPE, stderr=subprocess.PIPE) for _ in range(2)]
        snapshots = [json.loads(p.communicate(timeout=30)[0]) for p in reads]
        self.assertTrue(all(p.returncode == 0 for p in reads))
        self.assertEqual(len({s["generation"] for s in snapshots}), 2)
        latest = max(snapshots, key=lambda s: s["generation"])
        self.assertEqual(self.db()["generation"], latest["generation"])
        self.assertEqual(latest["sources"][0]["tasks"][0]["instance_token"], winner["instance_token"])

    def test_wrapped_reuse_crash_window_failed_creation(self):
        old = self.create()
        self.discard()
        self.assertEqual(self.rows(), {})
        self.assertEqual(self.db()["sources"][self.namespace]["slots"], {})
        fresh = self.create(pane="pane-b")
        self.assertNotEqual(fresh["instance_token"], old["instance_token"])
        # Leave a published origin/cache behind, remove externally, then crash-create
        # without an intervening snapshot that might conveniently retire the old slot.
        self.row()
        self.command([BINARY, "discard", "t-one"], self.repo)
        self.assertEqual(self.db()["sources"][self.namespace]["slots"]["t-one"]["instance_token"], fresh["instance_token"])
        self.cli("create", "--checkout", str(self.repo), "--id", "t-one", "--pane", "pane-a", "--test-crash-after-new", code=86)
        crashed = self.row()
        self.assertIsNone(crashed["instance_token"])
        self.assertIsNone(crashed["origin"])
        self.assertEqual(crashed["health"], "live")
        self.create(code=3)
        self.assertIsNone(self.row()["origin"])
        self.discard()
        self.control({"new_fail": True})
        self.create(code=3)
        self.assertEqual(self.db()["sources"][self.namespace]["slots"]["t-one"], {"state": "pending"})
        self.control({})
        self.assertEqual(self.rows(), {})
        self.assertEqual(self.db()["sources"][self.namespace]["slots"], {})
        # An external creation after absence preflight must not acquire the fresh
        # origin when our delegate new reports failure.
        self.control({"new_race": True})
        self.create(pane="pane-b", code=3)
        self.control({})
        raced = self.row()
        self.assertEqual(raced["health"], "live")
        self.assertIsNone(raced["origin"])
        self.assertIsNone(raced["instance_token"])

    def test_observed_disappearance_reappearance_is_unknown(self):
        self.create()
        self.write_events("t-one", ["old detail"])
        self.row()
        # Deliberately unsupported direct lifecycle calls simulate external reuse.
        self.command([BINARY, "discard", "t-one"], self.repo)
        self.assertEqual(self.rows(), {})
        self.command([BINARY, "new", "t-one"], self.repo)
        row = self.row()
        self.assertIsNone(row["origin"])
        self.assertIsNone(row["instance_token"])
        self.assertEqual(row["steps"], [])

    def test_degraded_reads_disconnect_reconcile_and_caps(self):
        self.create("t-good")
        self.create("t-bad", "pane-b")
        self.write_events("t-good", ["s" + str(i) for i in range(8)])
        all_row = self.row("t-good")
        self.assertEqual(len(all_row["steps"]), 5)
        self.assertIsNone(all_row["steps_truncated"])
        self.row("t-bad")
        self.control({"all": "fail", "t-bad": "fail"})
        snap = self.snap()["sources"][0]
        rows = {t["task_id"]: t for t in snap["tasks"]}
        self.assertEqual(snap["mode"], "registered_fallback")
        self.assertEqual(snap["health"], "stale")
        self.assertEqual(rows["t-good"]["health"], "live")
        self.assertTrue(rows["t-good"]["steps_truncated"])
        self.assertEqual(rows["t-bad"]["health"], "stale")
        self.create("t-no-preflight", code=3)
        self.assertNotIn("t-no-preflight", self.db()["sources"][self.namespace]["slots"])
        # Missing in a degraded single read must not retire the stale task.
        self.command([BINARY, "discard", "t-bad"], self.repo)
        self.assertEqual(self.row("t-bad")["health"], "stale")
        self.control({"all": "malformed", "t-good": "fail", "t-bad": "fail"})
        self.assertTrue(all(t["health"] == "stale" for t in self.snap()["sources"][0]["tasks"]))
        self.control({})
        reconciled = self.rows()
        self.assertEqual(set(t for _, t in reconciled), {"t-good"})
        repeat = self.rows()
        self.assertEqual(reconciled, repeat)
        self.api.mode = "disconnect"
        before = self.db()["sources"][self.namespace]["slots"]
        self.create("t-disconnected", code=3)
        self.assertEqual(self.db()["sources"][self.namespace]["slots"], before)
        self.api.mode = "normal"
        self.create("t-reconnected")
        self.assertIsNotNone(self.row("t-reconnected")["origin"])

    def test_unavailable_without_cache_and_real_malformed_task_isolation(self):
        self.create("t-good")
        self.create("t-bad")
        bad = self.taskdir("t-bad") / "STATE.json"
        bad.write_text("broken JSON")
        snap = self.snap()["sources"][0]
        self.assertEqual(snap["mode"], "registered_fallback")
        rows = {t["task_id"]: t for t in snap["tasks"]}
        self.assertEqual(rows["t-good"]["health"], "live")
        self.assertEqual(rows["t-bad"]["health"], "unavailable")
        self.assertIsNone(rows["t-bad"]["phase"])
        bad.unlink()
        self.assertEqual(self.row("t-bad")["health"], "live")

    def test_reset_inert_oversized_and_malformed_records(self):
        self.create()
        self.create("t-healthy", "pane-b")
        self.write_events("t-one", ["previous-result"])
        old = self.row()
        self.write_events("t-one", ["new-attempt"])
        new = self.row()
        self.assertEqual(old["steps"][0]["id"], new["steps"][0]["id"])
        self.assertEqual([s["target"] for s in new["steps"]], ["new-attempt"])
        self.assertFalse(new["steps_truncated"])
        sentinel = self.root / "must-not-exist"
        hostile = "\x1b]2;title\x07\u202e$(touch " + str(sentinel) + ");" + "x" * 20000
        self.write_events("t-one", [hostile, "valid-after"])
        with (self.taskdir() / "events.jsonl").open("a") as stream:
            stream.write('malformed\n{"id":123,"kind":"run","target":false,"at":"x"}\n')
        row = self.row()
        self.assertTrue(row["detail_limited"])
        self.assertTrue(any(s["target"] == "valid-after" for s in row["steps"]))
        self.assertFalse(sentinel.exists())
        self.assertNotIn("\x1b", json.dumps(row))
        self.assertEqual(self.row("t-healthy")["health"], "live")
        # Bounded text on a valid step is sanitized; excessive record is rejected.
        self.write_events("t-one", ["\x1b[31m$(no-execution)" + "a" * 5000])
        row = self.row()
        self.assertTrue(row["detail_limited"])
        self.assertEqual(len(row["steps"][0]["target"]), 4096)
        self.assertNotIn("\x1b", row["steps"][0]["target"])
        # Output overflow cannot become empty authoritative state; healthy fallback.
        self.control({"all": "oversized"})
        self.assertEqual(self.row("t-healthy")["health"], "live")
        self.control({})
        self.write_events("t-one", ["recovered"])
        self.assertEqual(self.row()["steps"][0]["target"], "recovered")

    def test_malformed_read_boundary_record_is_local(self):
        self.create("t-good")
        self.create("t-noisy", "pane-b")
        response = json.loads(self.command([BINARY, "status", "--json"], self.repo).stdout)
        noisy = next(t for t in response["tasks"] if t["id"] == "t-noisy")
        noisy["title"] = "\x1b[31m" + "x" * 6000
        noisy["steps_available"] = True
        noisy["steps"] = [None, {"id": 1, "at": "synthetic", "kind": "future-unsupported", "target": "bad", "result": None}, {"id": 2, "at": "synthetic", "kind": "read", "target": "valid-after-malformed", "result": None}]
        self.control({"all": response})
        rows = self.rows()
        row = rows[(self.namespace, "t-noisy")]
        self.assertTrue(row["detail_limited"])
        self.assertEqual(row["rejected_steps"], 2)
        self.assertEqual(len(row["title"]), 4096)
        self.assertNotIn("\x1b", row["title"])
        self.assertEqual([s["target"] for s in row["steps"]], ["valid-after-malformed"])
        self.assertEqual(rows[(self.namespace, "t-good")]["health"], "live")
        self.control({})
        self.assertEqual(self.row("t-noisy")["rejected_steps"], 0)
        self.assertFalse(self.row("t-noisy")["detail_limited"])

    def test_entire_source_unavailable_without_prior_observation(self):
        self.create()
        self.control({"all": "fail", "t-one": "fail"})
        source = self.snap()["sources"][0]
        self.assertEqual(source["health"], "unavailable")
        self.assertEqual(source["read_generation"], 0)
        self.assertEqual(source["tasks"][0]["health"], "unavailable")
        self.assertIsNone(source["tasks"][0]["steps_available"])
        self.assertEqual(len(self.db()["sources"][self.namespace]["slots"]), 1)
        self.control({})
        self.assertEqual(self.row()["health"], "live")

    def test_repeated_snapshots_only_status_and_task_state_unchanged(self):
        self.create()
        self.run_worker()  # Wait for synchronous stand-in exit, no heartbeat races.
        task = self.taskdir()
        def evidence():
            files = {name: (task / name).read_bytes() if (task / name).exists() else None for name in ("STATE.json", "status.json", "events.jsonl")}
            files["HEAD"] = self.command(["git", "rev-parse", "HEAD"], task / "worktree").stdout
            return files
        before = evidence()
        self.calls.write_text("")
        api_count = len(self.api.requests)
        first = self.rows()
        for _ in range(3):
            self.assertEqual(self.rows(), first)
        self.assertEqual(evidence(), before)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(calls, [["status", "--json"]] * 4)
        self.assertEqual(len(self.api.requests), api_count)
        self.assertEqual(self.api.errors, [])
        self.assertTrue(all("log" not in row for row in first.values()))

    def test_gate_failure_classification(self):
        def code(*exceptions):
            def fail(exc):
                def run():
                    raise exc
                return unittest.FunctionTestCase(run)
            suite = unittest.TestSuite(fail(exc) for exc in exceptions)
            result = unittest.TextTestRunner(stream=io.StringIO(), resultclass=SmokeResult).run(suite)
            return result.exit_code()
        self.assertEqual(code(), 0)
        self.assertEqual(code(SetupFailure("tool")), 3)
        for exc in (ValueError("bad JSON"), KeyError("missing row"), RuntimeError("product"),
                    FileNotFoundError("missing product evidence"), AssertionError("product")):
            self.assertEqual(code(exc), 1)
            self.assertEqual(code(SetupFailure("tool"), exc), 1)

    def test_read_envelope_and_duplicates_remain_structural_failures(self):
        self.create()
        response = json.loads(self.command([BINARY, "status", "--json"], self.repo).stdout)
        for broken in ({"tasks": "not a list"}, {"tasks": [None]},
                       {"tasks": response["tasks"] * 2}):
            self.control({"all": broken})
            source = self.snap()["sources"][0]
            self.assertEqual(source["mode"], "registered_fallback")
            self.assertEqual(source["tasks"][0]["health"], "live")

    # Review evidence (t123 adversarial review): expected to fail on round 1.
    def test_review_valid_delegate_id_does_not_poison_full_feed(self):
        self.create("t-one")
        # delegate's Task::load accepts this id (only empty, leading _/. and
        # slashes are rejected); it is a well-formed record, not malformed data.
        self.command([BINARY, "new", "fix+login"], self.repo)
        source = self.snap()["sources"][0]
        self.assertEqual(source["mode"], "all")
        self.assertIn("fix+login", {t["task_id"] for t in source["tasks"]})
        # The absence preflight must not be blocked by an unrelated task.
        self.create("t-two", "pane-b")

    def test_review_linked_alias_run_uses_linked_lead_branch(self):
        linked = self.root / "linked"
        self.command(["git", "worktree", "add", "-qb", "linked-lead", linked], self.repo)
        self.cli("register", "--checkout", str(linked))
        self.cli("create", "--checkout", str(linked), "--id", "t-linked", "--pane", "pane-a")
        self.cli("run", "--checkout", str(linked), "--id", "t-linked")
        state = json.loads((self.taskdir("t-linked") / "STATE.json").read_text())
        # delegate run from the linked lead bases the task on that lead's branch.
        self.assertEqual(state["base"], "linked-lead")

    def test_review_r2_hostile_task_id_is_not_emitted_with_controls(self):
        import unicodedata
        # Preserve opaque identity even where safe display strings collide.
        hostile = "t\x1b]2;pwn\x07\u202eabc"
        collision = "t\ufffd]2;pwn\ufffd\ufffdabc"
        for task_id in (hostile, collision):
            self.command([BINARY, "new", task_id], self.repo)
        source = self.snap()["sources"][0]
        self.assertEqual(source["mode"], "all")
        rows = {t["task_id"]: t for t in source["tasks"]}
        self.assertEqual(set(rows), {hostile, collision})
        for task_id, task in rows.items():
            self.assertEqual(task["display_task_id"], collision)
            self.assertEqual(task["task_id_limited"], task_id == hostile)
            self.assertFalse(any(unicodedata.category(c).startswith("C")
                                 for c in task["display_task_id"]))
        self.control({"all": "fail"})
        stale = self.rows()
        self.assertEqual(stale[(self.namespace, hostile)]["health"], "stale")
        self.assertEqual(stale[(self.namespace, hostile)]["display_task_id"], collision)
        # Fresh malformed detail produces unavailable rows, not unsafe labels.
        db = self.db()
        db["sources"][self.namespace]["cache"] = {}
        (self.state / "state.json").write_text(json.dumps(db))
        long_id = "t" + "x" * 4096
        self.control({"all": {"tasks": [{"id": k} for k in (hostile, collision, long_id)]}})
        rows = self.rows()
        self.assertEqual(len(rows), 3)
        for task_id in (hostile, collision, long_id):
            task = rows[(self.namespace, task_id)]
            self.assertEqual(task["health"], "unavailable")
            self.assertLessEqual(len(task["display_task_id"]), 4096)
            self.assertEqual(task["task_id_limited"], task_id != collision)
            self.assertFalse(any(unicodedata.category(c).startswith("C")
                                 for c in task["display_task_id"]))

    def test_review_r2_register_regression_is_product_failure(self):
        # init/register are product operations; a regression in them during
        # setUp must be a product failure (exit 1), not a setup failure (exit 3).
        for operation in ("init", "register"):
            for error in (AssertionError("product regression"), ValueError("bad JSON"),
                          KeyError("missing result row")):
                with self.subTest(operation=operation, error=type(error).__name__):
                    class Broken(Smoke):
                        def cli(inner, *args, code=0, env=None):
                            if args and args[0] == operation:
                                raise error
                            return Smoke.cli(inner, *args, code=code, env=env)
                    case = Broken("test_fixture_contract_and_explicit_setup")
                    result = unittest.TextTestRunner(stream=io.StringIO(), resultclass=SmokeResult).run(case)
                    self.assertEqual(result.exit_code(), 1)

    def test_private_provisioning_classification(self):
        from unittest.mock import patch
        with patch.object(socket.socket, "bind", side_effect=OSError("fixture bind")):
            with self.assertRaises(SetupFailure):
                FixtureAPI(self.root / "failed.sock")
        for operation in ("init", "add", "commit"):
            original = self.command
            def fail(argv, *args, **kwargs):
                if argv[:2] == ["git", operation]:
                    raise subprocess.CalledProcessError(1, argv)
                return original(argv, *args, **kwargs)
            with patch.object(self, "command", side_effect=fail):
                with self.assertRaises(SetupFailure):
                    self.make_repo("broken-" + operation)
        for error in (AssertionError("product"), ValueError("JSON"), KeyError("row")):
            with self.assertRaises(type(error)):
                with provisioning("fixture"):
                    raise error

    def test_discard_crash_and_failure_never_restore_origin(self):
        self.create()
        self.row()
        self.cli("discard", "--checkout", str(self.repo), "--id", "t-one",
                 "--test-crash-after-discard", code=86)
        self.command([BINARY, "new", "t-one"], self.repo)
        row = self.row()
        self.assertIsNone(row["origin"])
        self.assertIsNone(row["instance_token"])
        self.assertEqual(self.db()["sources"][self.namespace]["slots"]["t-one"], {"state": "retiring"})
        self.discard()
        self.create()
        # A deterministic ambiguous removal must also lose attribution.
        self.control({"discard_fail": True})
        self.cli("discard", "--checkout", str(self.repo), "--id", "t-one", code=3)
        self.assertIsNone(self.row()["origin"])
        self.assertIsNone(self.row()["instance_token"])

    # Review evidence (t123 adversarial review, round 3).
    def test_review_r3_recreated_checkout_path_keeps_old_namespace_and_origin(self):
        import shutil
        old = self.create()
        self.row()  # cache old detail before replacement
        before = self.db()["sources"]
        shutil.rmtree(self.repo)
        self.repo = self.make_repo("checkout")
        # Even explicit registration cannot adopt a replacement alias.
        self.cli("register", "--checkout", str(self.repo), code=3)
        self.assertEqual(self.db()["sources"], before)
        for operation in ("run", "discard"):
            self.cli(operation, "--checkout", str(self.repo), "--id", "t-one", code=3)
        self.create(code=3)
        self.command([BINARY, "new", "t-one"], self.repo)
        self.calls.write_text("")
        row = self.row()
        self.assertEqual(row["health"], "stale")
        self.assertEqual(row["instance_token"], old["instance_token"])
        self.assertEqual(self.calls.read_text(), "")
        (self.repo / ".git/delegate-checkout-id").unlink()
        self.cli("register", "--checkout", str(self.repo), code=3)
        self.create(code=3)
        self.assertEqual(self.row()["health"], "stale")
        db = self.db()
        db["sources"][self.namespace]["cache"] = {}
        (self.state / "state.json").write_text(json.dumps(db))
        self.assertEqual(self.row()["health"], "unavailable")
        self.assertEqual(self.calls.read_text(), "")

    def test_review_r3_all_feed_over_cap_is_not_reported_untruncated(self):
        self.create()
        response = json.loads(self.command([BINARY, "status", "--json"], self.repo).stdout)
        response["tasks"][0]["steps_available"] = True
        response["tasks"][0]["steps"] = [{"id": i, "at": "synthetic", "kind": "read", "target": str(i), "result": None} for i in range(1, 9)]
        self.control({"all": response})
        row = self.row()
        self.assertEqual(len(row["steps"]), 5)
        # The adapter itself dropped three received records; false is a false claim.
        self.assertIs(row["steps_truncated"], True)

    # Review evidence (t123 adversarial review, round 4).
    def test_review_r4_relocated_checkout_is_not_adopted_unobservably(self):
        import shutil
        self.create()
        moved = self.root / "moved"
        shutil.move(str(self.repo), str(moved))
        before = self.db()["sources"]
        self.cli("register", "--checkout", str(moved), code=3)
        self.assertEqual(self.db()["sources"], before)
        self.create("t-two", repo=moved, code=3)
        self.assertEqual(self.row()["health"], "unavailable")
        self.assertNotIn("t-two", [t["task_id"] for t in self.snap()["sources"][0]["tasks"]])

    def test_removed_pinned_linked_path_blocks_surviving_main_alias(self):
        linked = self.root / "first-linked"
        self.command(["git", "worktree", "add", "-qb", "first-lead", linked], self.repo)
        # A separate experiment pins the linked worktree first, then adds main.
        self.state = self.root / "linked-experiment"
        self.cli("init", "--delegate", str(self.launcher), "--socket", str(self.socket))
        self.namespace = self.cli("register", "--checkout", str(linked))["checkout_namespace"]
        self.cli("register", "--checkout", str(self.repo))
        self.create()
        self.row()  # preserve old detail only as stale after removal
        self.command(["git", "worktree", "remove", "--force", linked], self.repo)
        before = self.db()["sources"]
        self.calls.write_text("")
        self.cli("register", "--checkout", str(self.repo), code=3)
        self.create("t-two", code=3)
        for operation in ("run", "discard"):
            self.cli(operation, "--checkout", str(self.repo), "--id", "t-one", code=3)
        self.assertEqual(self.db()["sources"], before)
        self.assertEqual(self.row()["health"], "stale")
        self.assertEqual(self.calls.read_text(), "")

    def test_review_r4_display_task_id_has_no_line_separators(self):
        import unicodedata
        hostile = "t\u2028spoofed-row\u2029x"
        self.command([BINARY, "new", hostile], self.repo)
        row = self.row(hostile)
        self.assertFalse(any(unicodedata.category(c) in ("Zl", "Zp") for c in row["display_task_id"]),
                         row["display_task_id"])
        self.assertTrue(row["task_id_limited"])
        self.assertEqual(row["task_id"], hostile)
        response = json.loads(self.command([BINARY, "status", "--json"], self.repo).stdout)
        response["tasks"][0].update(title=hostile, steps_available=True, steps=[
            {"id": 1, "at": "synthetic", "kind": "read", "target": hostile, "result": None}])
        self.control({"all": response})
        row = self.row(hostile)
        expected = hostile.replace("\u2028", "\ufffd").replace("\u2029", "\ufffd")
        self.assertEqual(row["display_task_id"], expected)
        self.assertEqual(row["title"], expected)
        self.assertEqual(row["steps"][0]["target"], expected)

    # Preserve the supplied adversarial regression for origin display safety.
    def test_review_r5_origin_is_not_emitted_with_line_separators(self):
        # Unsafe references must become unknown, not sanitized identity keys.
        self.api.panes["pane-a"]["agent_session"] = {
            "source": "herdr:claude", "agent": "claude\u2028spoofed-group",
            "kind": "id", "value": "synthetic\u2029reference"}
        self.create()
        origin = self.row()["origin"]
        self.assertIsNone(origin)

    def test_unsafe_origin_fields_capture_and_sidecar(self):
        base = copy.deepcopy(self.api.panes["pane-a"]["agent_session"])
        self.create()
        for field in ("source", "agent", "kind", "value"):
            for unsafe in ("\u2028", "\u2029", "\x1b", "\u202e"):
                with self.subTest(field=field, unsafe=repr(unsafe)):
                    origin = dict(base)
                    origin[field] += unsafe
                    self.api.panes["pane-a"]["agent_session"] = origin
                    self.assertIsNone(self.cli("capture", "--pane", "pane-a")["origin"])
                    # Previously published sidecars must obey the same emission rule.
                    db = self.db()
                    db["sources"][self.namespace]["slots"]["t-one"]["origin"] = origin
                    (self.state / "state.json").write_text(json.dumps(db))
                    self.assertIsNone(self.row()["origin"])


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Smoke)
    result = unittest.TextTestRunner(verbosity=2, resultclass=SmokeResult).run(suite)
    # Bad JSON, missing rows/files and ordinary exceptions are product failures.
    # Only explicitly marked fixture/tool setup errors are exit 3; product wins.
    sys.exit(result.exit_code())
