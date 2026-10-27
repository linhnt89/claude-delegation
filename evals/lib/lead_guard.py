"""Claude Code's documented hook protocol, not an implementation of the guard.

Used by the lead directly and by stand-ins inside work(). No worker identity is
invented here: hook subprocesses inherit exactly the caller's environment.
"""

import collections
import json
import os
from pathlib import Path
import random
import re
import subprocess
import sys
import time
import uuid

TOOLS = ("Edit", "Write", "NotebookEdit")
FAILURES = []


def expect(ok, message):
    if not ok:
        FAILURES.append(message)
        print(f"FAIL: {message}", flush=True)


def finish(message):
    if FAILURES:
        print(f"FAIL: {FAILURES[0]} ({len(FAILURES)} assertion(s) failed)")
        raise SystemExit(1)
    print(message)


def remaining():
    return max(0.01, float(os.environ.get("GUARD_DEADLINE", time.time() + 10)) - time.time())


class Hooks:
    def __init__(self, root, session=None):
        self.root = Path(root).resolve()
        self.session = session or f"guard-eval-{uuid.uuid4().hex}"
        # HOME is the check's scratch home, also writable in a worker sandbox.
        scratch = Path.home() / ".cache/stub"
        scratch.mkdir(parents=True, exist_ok=True)
        self.transcript = scratch / f"{self.session}.jsonl"
        self.transcript.touch()

    def call(self, event, *, tool=None, path=None, prompt="Continue the agreed plan.",
             active=False, raw=None, env=None):
        payload = {"session_id": self.session, "transcript_path": str(self.transcript),
                   "cwd": str(self.root), "hook_event_name": event}
        if event == "PreToolUse":
            payload.update(tool_name=tool, tool_input={
                "notebook_path" if tool == "NotebookEdit" else "file_path": str(path)})
        elif event == "UserPromptSubmit":
            payload["prompt"] = prompt
        elif event == "Stop":
            payload["stop_hook_active"] = active
        settings = self.root / ".claude/settings.json"
        config = json.loads(settings.read_text()) if settings.exists() else {}
        results = []
        for group in config.get("hooks", {}).get(event, []):
            matcher = group.get("matcher", "")
            if event == "PreToolUse" and matcher and not re.search(matcher, tool or ""):
                continue
            for hook in group.get("hooks", []):
                if hook.get("type") != "command":
                    continue
                child_env = dict(os.environ if env is None else env)
                child_env["CLAUDE_PROJECT_DIR"] = str(self.root)
                try:
                    p = subprocess.run(["sh", "-c", hook["command"]], cwd=self.root,
                                       env=child_env, input=raw if raw is not None else json.dumps(payload),
                                       capture_output=True, text=True, timeout=min(5, remaining()))
                except subprocess.TimeoutExpired:
                    expect(False, f"{event} guard hook timed out for {path or self.session}")
                    results.append({"blocked": False, "reason": "", "messages": "", "error": True})
                    continue
                output = {}
                if p.returncode == 0 and p.stdout.strip():
                    try:
                        output = json.loads(p.stdout)
                        if not isinstance(output, dict):
                            raise ValueError("not an object")
                    except (ValueError, TypeError):
                        expect(False, f"{event} hook returned unreadable verdict: {p.stdout[:160]!r}")
                specific = output.get("hookSpecificOutput") or {}
                denied = (event == "PreToolUse" and
                          specific.get("hookEventName") == event and
                          specific.get("permissionDecision") == "deny")
                held = event == "Stop" and output.get("decision") == "block"
                blocked = p.returncode == 2 or (p.returncode == 0 and (denied or held))
                reason = (p.stderr if p.returncode == 2 else
                          specific.get("permissionDecisionReason", "") if denied else
                          output.get("reason", "") if held else "")
                expect(p.returncode in (0, 2),
                       f"{event} guard hook exited {p.returncode}, not a verdict: {p.stderr[:160]}")
                messages = [p.stderr, output.get("reason", ""), output.get("systemMessage", ""),
                            specific.get("additionalContext", ""), specific.get("permissionDecisionReason", "")]
                results.append({"blocked": blocked, "reason": reason,
                                "messages": "\n".join(str(m) for m in messages if m),
                                "error": p.returncode not in (0, 2)})
        return {"blocked": any(r["blocked"] for r in results),
                "reason": "\n".join(r["reason"] for r in results if r["blocked"]),
                "messages": "\n".join(r["messages"] for r in results if r["messages"]),
                "error": any(r["error"] for r in results)}

    def start(self, *, env=None, who="lead"):
        result = self.call("UserPromptSubmit", env=env)
        expect(not result["blocked"] and not result["error"], f"{who} turn start was refused")

    def edit(self, tool, path, allowed, *, content="# guard eval edit\n", restore=True,
             env=None, who="lead"):
        target = (self.root / path).resolve()
        before = target.read_bytes() if target.exists() else None
        verdict = self.call("PreToolUse", tool=tool, path=path, env=env)
        # Claude Code, not the hook, performs an allowed edit.
        if not verdict["blocked"] and not verdict["error"]:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
        expect(verdict["blocked"] == (not allowed) and not verdict["error"],
               f"{tool} of {path} by the {who} was "
               f"{'refused' if verdict['blocked'] else 'allowed'}; expected "
               f"{'allowed' if allowed else 'refused'}")
        if not allowed:
            expect((target.read_bytes() if target.exists() else None) == before,
                   f"refused {tool} changed {path}")
            if verdict["blocked"]:
                refusal(verdict, path, self.root)
        if restore:
            if before is None:
                target.unlink(missing_ok=True)
            else:
                target.write_bytes(before)
        return verdict


def refusal(verdict, path, root):
    resolved = (root / path).resolve()
    names = [str(path), str(resolved)]
    if resolved.is_relative_to(root):
        names.append(str(resolved.relative_to(root)))
    expect(any(n in verdict["reason"] for n in names),
           f"refusal did not name {path}: {verdict['reason']!r}")
    expect(bool(re.search(r"\bspec(?:ification)?\b", verdict["reason"], re.I)) and
           "/delegate" in verdict["reason"],
           f"refusal for {path} did not say to write a spec and /delegate it: {verdict['reason']!r}")


def fixtures(root):
    root = Path(root)
    for p, body in {"PLAN.md": "# Agreed plan\n", "docs/decisions/009-x.md": "# Decision\n",
                    "evals/jobs.md": "# Jobs\n", "shop/cart.py": "# Cart\n",
                    "evals/scenarios/S001-shop/scenario.md": "# Shop scenario\n",
                    "evals/scenarios/S001-shop/check": "#!/bin/sh\nexit 0\n",
                    "notebooks/prices.ipynb": '{"cells":[],"nbformat":4,"nbformat_minor":5}\n'}.items():
        dest = root / p
        dest.parent.mkdir(parents=True, exist_ok=True)
        if not dest.exists():
            dest.write_text(body)
    link = root / "docs/link"
    if not link.is_symlink():
        link.symlink_to("../shop", target_is_directory=True)


def property_cases(hooks, worker=False):
    """Generate from labelled examples of the scenario's paths, not guard code."""
    fixtures(hooks.root)
    outside = Path.home() / ".cache/stub/outside scratch"
    outside.mkdir(exist_ok=True)
    (hooks.root / "shop/outside").symlink_to(outside, target_is_directory=True)
    (hooks.root / "shop/lead-docs").symlink_to("../docs", target_is_directory=True)
    rng = random.Random(int(os.environ["EVALS_SEED"]))
    # Labels are the scenario's oracle. New names exercise nesting, spaces and dotfiles.
    paths = [("PLAN.md", True), ("CLAUDE.md", True), ("AGENTS.md", True),
             ("evals/jobs.md", True), ("evals/scenarios/S001-shop/scenario.md", True),
             ("shop/pricing.py", False), ("tests/test_split.py", False),
             (".delegate.toml", False), (".claude/settings.json", False),
             ("evals/run", False), ("evals/scenarios/S001-shop/check", False),
             ("docs/../shop/pricing.py", False), ("docs/link/pricing.py", False),
             ("shop/lead-docs/notes.md", True), ("shop/outside/notes.py", True),
             (str(outside / "scratch.py"), True)]
    for _ in range(12):
        name = rng.choice(["price calc", ".local", "api/v2", "nested folder/cart"])
        suffix = rng.choice([".py", ".md", ".ipynb", ".sh"])
        paths.extend([(f"shop/{name}{suffix}", False), (f"docs/{name}{suffix}", True)])
    cases = []
    for path, lead_allowed in paths:
        for tool in TOOLS:
            spelling = rng.choice([path, str(hooks.root / path), f"./{path}" if not Path(path).is_absolute() else path])
            cases.append((tool, spelling, True if worker else lead_allowed))
    rng.shuffle(cases)
    # Shorter cases first: a failure is a single independent edit, already shrunk
    # to one operation; report the shortest generated counterexample as well.
    bad = []
    for tool, path, allowed in sorted(cases, key=lambda c: (len(c[1]), c[1], c[0])):
        count = len(FAILURES)
        if remaining() < 0.1:
            expect(False, "guard property exceeded its time budget")
            break
        hooks.edit(tool, path, allowed, who="worker" if worker else "lead")
        if len(FAILURES) != count:
            bad.append((tool, path, allowed))
    if bad:
        print(f"smallest failing case (single edit, shortest generated path): {bad[0]!r}", flush=True)
    return bad


def preserve_settings(before, after):
    for key, value in before.items():
        if key != "hooks":
            expect(after.get(key) == value, f"guard installation changed the user's setting {key}")
    for event, groups in before.get("hooks", {}).items():
        def entries(gs):
            return collections.Counter((g.get("matcher", ""), json.dumps(h, sort_keys=True))
                                       for g in gs for h in g.get("hooks", []))
        expect(not (entries(groups) - entries(after.get("hooks", {}).get(event, []))),
               f"guard installation changed the user's {event} hooks")


def worker(property_mode=False):
    hooks = Hooks(Path.cwd())
    hooks.start(who="worker")
    if property_mode:
        property_cases(hooks, worker=True)
    else:
        hooks.edit("Edit", "shop/pricing.py", True, restore=False, who="worker",
                   content=(hooks.root / "shop/pricing.py").read_text() + "\n# worker pricing change\n")
        hooks.edit("Write", "tests/test_split.py", True, restore=False, who="worker",
                   content="# worker split test\n")
    stop = hooks.call("Stop")
    expect(not stop["blocked"] and not stop["error"],
           f"worker's turn was held; expected it to end: {stop['reason']}")
    receipt = Path.home() / ".cache/stub/guard-worker.json"
    receipt.write_text(json.dumps({"failures": FAILURES, "cwd": str(hooks.root)}))
    (hooks.root / ".task/REPORT.md").write_text("# Worker hook probe\n\n" +
        ("\n".join(FAILURES) if FAILURES else "Edits allowed; turn ended.\n"))
    finish("worker edits allowed and turn ended")


if __name__ == "__main__":
    worker(property_mode=sys.argv[1:] == ["worker-property"])
