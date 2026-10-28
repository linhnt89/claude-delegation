"""Checks for decisions 016/017: S080, S081 and property P021.

Drives the real binary in $EVALS_WORK: tasks are held "running" by a stand-in worker that
blocks in its work phase (`wait_flag`), accounts and runway come from the stand-in vendors
(decision 014), and cleanup kills the runs it started and discards their tasks.

The property's model is the scenario's: a candidate covers the size when its account runway,
divided by (its running tasks + 1), is at least the size's `[quota.runway]`; the first such
candidate in tier order is the one that runs. "Runway" is the account report `delegate quota
--json` shows (the same reading selection uses).
"""

from __future__ import annotations

from datetime import datetime
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, os.environ["EVALS_LIB"])
from kit import d, dhome, seed, spec  # noqa: E402
from quota_checks import (  # noqa: E402
    claude_body, codex_body, copilot_body, find_json_object, make_claude_login,
    make_codex_login, make_pi_login, route, routes_for, worker_calls,
)
from vendors import VendorServer  # noqa: E402

HOME = Path(os.environ["HOME"])
WORK = Path(os.environ["EVALS_WORK"])
PROJ = Path(os.environ["PROJ"])
DELEGATE = os.environ["DELEGATE"]

REQUIRED = {"small": 30 * 60, "medium": 60 * 60, "long": 3 * 60 * 60}
SIZES = ("small", "medium", "long")
ACCOUNTS = ("claude", "codex", "copilot", "hyper")
CANDIDATES = {
    "claude": "claude:claude-worker@high",
    "codex": "codex:codex-worker@high",
    "copilot": "pi:github-copilot/copilot-worker@high",
    "hyper": "pi:hyper/hyper-worker@high",
}
TIME = re.compile(r"\b\d+h\d+m\b"
                  r"|\b\d+(?:\.\d+)?\s*(?:h|hr|hrs|hour|hours|m|min|mins|minute|minutes)\b"
                  r"|\b\d{1,2}:\d{2}\b", re.I)
CANDIDATE = re.compile(r"(?m)^\s*candidate:\s*(\S+)")
SHARE_WORDS = ("shared", "share", "divid", "per task", "each task", "per running", "split")
CASE: str | None = None


class CheckError(Exception):
    """The check could not construct or inspect its own fixture."""


def check_error(message: str) -> None:
    print("CHECK ERROR: " + " ".join(str(message).split())[:700], flush=True)
    raise SystemExit(3)


def fail(message: str) -> None:
    if CASE:
        print(f"smallest failing case: {CASE}", flush=True)
    print("FAIL: " + " ".join(str(message).split())[:900], flush=True)
    raise SystemExit(1)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


# --------------------------------------------------------------- driving the CLI


def bg(*args: str) -> tuple[int, str]:
    """`delegate run --background ...`, as a lead would start a task."""
    try:
        p = subprocess.run([DELEGATE, "run", "--background", *args], cwd=PROJ,
                           capture_output=True, text=True, stdin=subprocess.DEVNULL,
                           timeout=30, check=False)
    except subprocess.TimeoutExpired:
        raise CheckError(f"delegate run --background {' '.join(args)} did not return in 30s")
    out = p.stdout + p.stderr
    with open(WORK / "transcript.log", "a", encoding="utf-8") as log:
        log.write(f"\n$ delegate run --background {' '.join(args)}  (exit {p.returncode})\n{out}")
    return p.returncode, out


def task_status(tid: str) -> dict[str, Any] | None:
    _rc, out = d("status", tid, "--json")
    decoder = json.JSONDecoder()
    for index, char in enumerate(out):
        if char != "{":
            continue
        try:
            value, _end = decoder.raw_decode(out[index:])
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and isinstance(value.get("tasks"), list) and value["tasks"]:
            return value["tasks"][0]
    return None


def wait_phase(tid: str, phase: str = "implement", timeout: float = 20.0) -> dict[str, Any] | None:
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        task = task_status(tid)
        if task and task.get("phase") == phase:
            return task
        time.sleep(0.1)
    return None


def kill_task(tid: str) -> None:
    task = task_status(tid)
    pid = task.get("pid") if task else None
    if not isinstance(pid, int):
        return
    for killer in (lambda: os.killpg(pid, signal.SIGKILL), lambda: os.kill(pid, signal.SIGKILL)):
        try:
            killer()
        except ProcessLookupError:
            pass


def cleanup(tids: list[str]) -> None:
    """Stop the runs this check started and remove their task state."""
    for tid in tids:
        kill_task(tid)
    time.sleep(0.3)
    for tid in tids:
        d("discard", tid)


def start_running(tid: str, worker: str | None = None, size: str | None = None) -> str:
    """Start a spec in the background and wait until its worker is in the work phase."""
    if not spec_path(tid).is_file():
        spec(tid, title=f"hold {tid} running")
        if size:
            append_size(tid, size)
    args = (["--worker", worker] if worker else []) + [tid]
    rc, out = bg(*args)
    if rc != 0:
        raise CheckError(f"could not start {tid}: {out[-400:]}")
    task = wait_phase(tid)
    if task is None:
        raise CheckError(f"{tid} never reached its work phase: {out[-400:]}")
    return out


def spec_path(tid: str) -> Path:
    return Path(dhome()) / tid / "SPEC.md"


def append_size(tid: str, size: str) -> None:
    path = spec_path(tid)
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise CheckError(f"no spec for {tid}: {exc}") from exc
    path.write_text(text.rstrip() + f"\nsize: {size}\n", encoding="utf-8")


def candidates(output: str) -> list[str]:
    return CANDIDATE.findall(output)


def account_of(worker: str) -> str | None:
    if worker.startswith("claude:"):
        return "claude"
    if worker.startswith("codex:"):
        return "codex"
    if worker.startswith("pi:openai-codex/"):
        return "codex"
    if worker.startswith("pi:github-copilot/"):
        return "copilot"
    if worker.startswith("pi:hyper/"):
        return "hyper"
    return None


def list_line(output: str, tid: str) -> str:
    return next((line for line in output.splitlines() if line.startswith(tid)), "")


# --------------------------------------------------------------- runway evidence


def share_evidence(output: str) -> str | None:
    """A line naming an account, its remaining runway and that it is shared."""
    for line in output.splitlines():
        low = line.lower()
        if "codex" in low and TIME.search(line) and any(word in low for word in SHARE_WORDS):
            return line
    return None


def skip_evidence(output: str) -> str | None:
    """The line saying the first candidate is skipped because its runway is shared."""
    for line in output.splitlines():
        low = line.lower()
        if ("skip" in low and "codex" in low and "account" in low
                and any(word in low for word in SHARE_WORDS) and TIME.search(line)):
            return line
    return None


# --------------------------------------------------------------- S080


def check_s080() -> None:
    ids = ["t001", "t002", "t003"]
    started: list[str] = []
    make_codex_login("FAKE_S080_CODEX_ACCESS")
    make_pi_login(copilot_token="ghu_FAKE_S080_COPILOT_REFRESH",
                  include_codex=False, include_hyper=False)
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        # The first account's runway covers three medium tasks at once: 6h of runway at its
        # pace (the shared value for the fourth task is 6h / 4 = 1h30m).
        server.set_routes(routes_for(codex=route(body=codex_body(50, 7 * 3600, 13 * 3600)),
                                     copilot=route(body=copilot_body(10, 86_400))))
        spec("t004", title="the fourth task")
        try:
            for tid in ids:
                started.append(tid)
                start_running(tid)
            work_models = [model for model, phase in worker_calls() if phase == "work"]
            require(work_models and all(model == "codex-worker" for model in work_models),
                    f"not all three tasks ran on the tier's first candidate: {work_models}")
            require(not any(model.startswith(("copilot", "hyper")) for model, _phase in worker_calls()),
                    f"the second account was used while the first was healthy: {worker_calls()}")

            rc, listed = d("list")
            require(rc == 0, f"delegate list failed: {listed[-400:]}")
            for tid in ids:
                line = list_line(listed, tid)
                require("[codex:codex-worker@high]" in line,
                        f"{tid} did not run on the tier's first candidate: {line!r}")

            rc, dry = d("run", "t004", "--dry-run")
            require(rc == 0, f"delegate run t004 --dry-run failed: {dry[-500:]}")
            offered = candidates(dry)
            require(offered and account_of(offered[0]) == "codex",
                    f"the dry run did not offer the first account first: {dry[-500:]}")
            evidence = share_evidence(dry)
            require(evidence is not None,
                    "the dry run did not show the first account's runway shared by the three "
                    f"running tasks: {dry[-600:]}")
        finally:
            cleanup(started)
    print("S080 kept all three tasks on the tier's first candidate, left the second account "
          "alone, and showed its runway shared by the running tasks")


# --------------------------------------------------------------- S081


def check_s081() -> None:
    started: list[str] = []
    make_codex_login("FAKE_S081_CODEX_ACCESS")
    make_pi_login(copilot_token="ghu_FAKE_S081_COPILOT_REFRESH",
                  include_codex=False, include_hyper=False)
    spec("t001", title="first long task")
    append_size("t001", "long")
    spec("t002", title="second long task")
    append_size("t002", "long")
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        # The first account has 4.5h of runway (1.5x the 3h a long task needs) and the second
        # is healthy: shared with one running task the first has 2h15m, too little for long.
        server.set_routes(routes_for(codex=route(body=codex_body(50, 5 * 3600, int(9.5 * 3600))),
                                     copilot=route(body=copilot_body(10, 86_400))))
        try:
            rc, alone = d("run", "t002", "--dry-run")
            require(rc == 0, f"delegate run t002 --dry-run failed: {alone[-500:]}")
            offered = candidates(alone)
            require(offered and account_of(offered[0]) == "codex",
                    f"with nothing running, the long task did not use the first candidate: {alone[-500:]}")

            started.append("t001")
            start_running("t001")
            rc, listed = d("list")
            line = list_line(listed, "t001")
            require("codex:codex-worker@high" in line,
                    f"the first long task did not run on the first account: {line!r}")

            rc, dry = d("run", "t002", "--dry-run")
            require(rc == 0, f"delegate run t002 --dry-run failed: {dry[-500:]}")
            evidence = skip_evidence(dry)
            require(evidence is not None,
                    "the dry run did not skip the first candidate because its runway is shared "
                    f"with t001, naming the account and the runway left per task: {dry[-600:]}")
            require("t001" in dry.lower(), f"the dry run did not name t001 as the running task: {dry[-500:]}")
            offered = candidates(dry)
            require(offered and account_of(offered[0]) == "copilot",
                    f"the dry run did not move the task to the second candidate: {dry[-500:]}")

            started.append("t002")
            rc, run_out = bg("t002")
            require(rc == 0, f"delegate run t002 --background failed: {run_out[-500:]}")
            evidence = skip_evidence(run_out)
            require(evidence is not None,
                    "starting t002 did not say it skips the first candidate because its runway "
                    f"is shared with t001: {run_out[-600:]}")
            require("t001" in run_out.lower(), f"starting t002 did not name t001: {run_out[-500:]}")
            require(wait_phase("t002") is not None, "t002 never reached its work phase")

            rc, listed = d("list")
            line = list_line(listed, "t002")
            require("pi:github-copilot/copilot-worker@high" in line,
                    f"t002 did not run on the second candidate: {line!r}")
            models = [model for model, phase in worker_calls() if phase == "work"]
            require(models.count("codex-worker") == 1,
                    f"t002 also ran on the first account: {models}")
            require(models.count("copilot-worker") == 1,
                    f"t002's work did not run on the second candidate: {models}")
        finally:
            cleanup(started)
    print("S081 skipped the first account only while its runway was shared, naming the account "
          "and the runway left per task, and ran both long tasks on their own account")


# --------------------------------------------------------------- P021


def iso_seconds(text: str) -> float | None:
    try:
        return datetime.fromisoformat(text.replace("Z", "+00:00")).timestamp()
    except (TypeError, ValueError):
        return None


def runway_seconds(report: dict[str, Any]) -> float | None:
    """Seconds the account lasts at its pace, None for through reset, -1 for unknown."""
    runway = report.get("runway") or {}
    status = runway.get("status")
    if status == "through_reset":
        return None
    if status == "exhausted_now":
        return 0.0
    at = runway.get("at")
    if status == "projected_exhaustion" and at:
        seconds = iso_seconds(at)
        if seconds is not None:
            return max(0.0, seconds - time.time())
    return -1.0


def report_covers(report: dict[str, Any], running: int, size: str) -> tuple[bool, float, float]:
    """(covers, shared runway per task, margin to the size's required runway)."""
    seconds = runway_seconds(report)
    required = float(REQUIRED[size])
    if seconds is None:
        return True, float("inf"), float("inf")
    if seconds < 0:
        raise CheckError(f"account {report.get('account')!r} quota was unknown: {report}")
    shared = seconds / (running + 1)
    return shared >= required, shared, abs(shared - required)


def read_reports() -> dict[str, dict[str, Any]]:
    rc, out = d("quota", "--json")
    if rc != 0:
        raise CheckError(f"delegate quota --json failed: {out[-500:]}")
    record = find_json_object(out)
    if record is None:
        raise CheckError(f"delegate quota --json did not emit the account record: {out[-500:]}")
    accounts = record.get("accounts")
    if not isinstance(accounts, list):
        raise CheckError("delegate quota --json has no accounts array")
    return {str(a.get("account")): a for a in accounts if isinstance(a, dict)}


def workers_for(accounts: list[str]) -> None:
    candidates = [CANDIDATES[account] for account in accounts]
    text = (
        'planner_family = "claude"\n'
        "[defaults]\n"
        'implement = "standard"\n'
        'research = "fast"\n'
        'review = "standard"\n'
        'plan_review = "standard"\n'
        "[implement]\n"
        f"fast = [{', '.join(json.dumps(c) for c in candidates)}]\n"
        f"standard = [{', '.join(json.dumps(c) for c in candidates)}]\n"
        f"deep = [{', '.join(json.dumps(c) for c in candidates)}]\n"
        "[research]\n"
        'fast = ["pi:deepseek-research@medium"]\n'
        "[review]\n"
        'standard = ["pi:gpt-review@high", "pi:deepseek-review@high"]\n'
        "[plan_review]\n"
        'standard = ["pi:claude-plan@high", "pi:deepseek-plan@high"]\n'
        "[quota]\n"
        "check = true\n"
        "[quota.runway]\n"
        'small = "30m"\n'
        'medium = "1h"\n'
        'long = "3h"\n'
    )
    (HOME / ".config" / "delegate").mkdir(parents=True, exist_ok=True)
    (HOME / ".config" / "delegate" / "workers.toml").write_text(text, encoding="utf-8")


def claude_body_for(rng: Any, mode: str) -> dict[str, Any]:
    if mode == "healthy":
        return claude_body(10, 5, 4 * 3600, 6 * 86_400)
    if mode == "exhausted":
        return claude_body(100, 100, 4 * 3600, 86_400)
    # Use most of the 5-hour window with little left: a finite runway, not "through reset".
    return claude_body(rng.choice([50, 80]), 5, rng.choice([3 * 3600, 4 * 3600]), 6 * 86_400)


def codex_body_for(rng: Any, mode: str, target_minutes: int | None = None) -> dict[str, Any]:
    if mode == "healthy":
        return codex_body(5, 4 * 3600, 5 * 3600)
    if mode == "exhausted":
        return codex_body(100, 3 * 3600)
    target = (target_minutes if target_minutes is not None
              else rng.choice([20, 40, 60, 90, 120, 180, 240])) * 60
    used = rng.choice([20, 50, 80])
    elapsed = target * used / (100 - used)
    reset = target + rng.choice([300, 900, 1800])
    return codex_body(used, int(reset), int(elapsed + reset))


def copilot_body_for(rng: Any, mode: str) -> dict[str, Any]:
    if mode == "healthy":
        return copilot_body(rng.choice([5, 10]), 86_400)
    if mode == "exhausted":
        return copilot_body(100, 3 * 3600)
    return copilot_body(rng.choice([30, 50, 70]), rng.choice([3600, 6 * 3600, 86_400]))


def hyper_body_for(rng: Any, mode: str) -> dict[str, Any]:
    if mode == "healthy":
        return {"balance": 250}
    if mode == "exhausted":
        return {"balance": 0}
    return {"balance": rng.choice([40, 80, 120, 160, 200])}


def bodies_for(account: str, mode: str, rng: Any, target: int | None = None) -> dict[str, Any]:
    makers = {
        "claude": claude_body_for,
        "codex": codex_body_for,
        "copilot": copilot_body_for,
        "hyper": hyper_body_for,
    }
    if account == "codex":
        return codex_body_for(rng, mode, target)
    return makers[account](rng, mode)


def deterministic_case() -> dict[str, Any]:
    """A case where ignoring the running tasks picks the wrong account.

    The first account lasts 4h at its pace, so alone it covers a long (3h) task but once
    shared with one running task it has 2h; the healthy second account must take it.
    """
    rng = __import__("random").Random(0)
    return {
        "accounts": ["codex", "copilot"],
        "modes": {"codex": "finite", "copilot": "healthy"},
        "running": {"codex": 1, "copilot": 0},
        "size": "long",
        "bodies": {"codex": codex_body_for(rng, "finite", 240),
                   "copilot": copilot_body_for(rng, "healthy")},
    }


def random_case(rng: Any) -> dict[str, Any]:
    count = rng.randint(1, 4)
    accounts = sorted(rng.sample(list(ACCOUNTS), count), key=ACCOUNTS.index)
    running = {account: rng.randint(0, 2) for account in accounts}
    while sum(running.values()) > 3:
        account = rng.choice(accounts)
        if running[account]:
            running[account] -= 1
    modes = {account: rng.choice(["healthy", "exhausted", "finite", "finite"])
             for account in accounts[:-1]}
    modes[accounts[-1]] = "healthy"  # the last one always covers, so every case is valid
    if "codex" in accounts[:-1]:
        modes["codex"] = "finite"  # a precisely known finite runway in tier order
    bodies = {account: bodies_for(account, modes[account], rng) for account in accounts}
    order = list(accounts)
    rng.shuffle(order)  # tier order is the user's choice, not the generator's
    return {
        "accounts": accounts,
        "order": order,
        "modes": modes,
        "running": running,
        "size": rng.choice(SIZES),
        "bodies": bodies,
    }


def describe(case: dict[str, Any]) -> str:
    return (f"accounts={case['accounts']} order={case.get('order', case['accounts'])} "
            f"runway={case['modes']} running={case['running']} size={case['size']}")


def try_case(server: VendorServer, case: dict[str, Any], index: int,
             attempt: int) -> str:
    """Set one case up and check the pick. Returns passed, failed or regenerate."""
    global CASE
    accounts = case["accounts"]
    order = case.get("order", accounts)
    CASE = describe(case)
    body = case["bodies"]
    server.set_routes(routes_for(
        claude=route(body=body.get("claude", claude_body())),
        codex=route(body=body.get("codex", codex_body())),
        copilot=route(body=body.get("copilot", copilot_body())),
        hyper=route(body=body.get("hyper", {"balance": 228})),
    ))
    workers_for(order)
    reports = read_reports()
    missing = [account for account in accounts if account not in reports]
    if missing:
        raise CheckError(f"quota JSON omitted configured accounts {missing}: {sorted(reports)}")
    if any(runway_seconds(reports[account]) == -1.0 for account in accounts):
        raise CheckError(f"quota was unreadable for a scripted account: {reports}")

    expected = None
    for account in order:
        covers, _shared, margin = report_covers(reports[account], case["running"][account],
                                                case["size"])
        if covers and expected is None:
            expected = account
        if margin >= 0 and margin < 900:
            return "regenerate"  # too close to the threshold to judge reliably
    if expected is None:
        return "regenerate"

    started: list[str] = []
    try:
        for account in accounts:
            for number in range(case["running"][account]):
                tid = f"t{index:02d}{attempt:02d}{ACCOUNTS.index(account)}{number}"
                started.append(tid)
                start_running(tid, worker=CANDIDATES[account])
        tested = f"z{index:02d}{attempt:02d}"
        spec(tested, title="the new task")
        append_size(tested, case["size"])
        started.append(tested)
        rc, dry = d("run", tested, "--dry-run")
        require(rc == 0, f"delegate run {tested} --dry-run failed: {dry[-500:]}")
        offered = candidates(dry)
        require(offered, f"the dry run offered no candidate although {expected} covers the size: {dry[-600:]}")
        picked = account_of(offered[0])
        require(picked is not None, f"could not tell which account {offered[0]!r} uses")
        require(picked == expected,
                f"the dry run picked {picked} ({offered[0]}) but the first candidate whose shared "
                f"runway covers the size is {expected}: {dry[-600:]}")
    finally:
        cleanup(started)
    CASE = None
    return "passed"


def check_p021() -> None:
    rng, actual_seed = seed()
    make_claude_login("FAKE_P021_CLAUDE_ACCESS")
    make_codex_login("FAKE_P021_CODEX_ACCESS")
    make_pi_login(copilot_token="ghu_FAKE_P021_COPILOT_REFRESH",
                  hyper_access="FAKE_P021_HYPER_ACCESS",
                  hyper_refresh="FAKE_P021_HYPER_REFRESH",
                  include_codex=False)
    cases = 7
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        for index in range(cases):
            attempt = 0
            while True:
                attempt += 1
                case = deterministic_case() if index == 0 else random_case(rng)
                verdict = try_case(server, case, index, attempt)
                if verdict == "passed":
                    break
                if attempt >= 20:
                    check_error("could not generate a case far enough from a runway threshold: "
                                + describe(case))
    print(f"P021 checked {cases} seeded cases against the shared-runway model (seed {actual_seed})")


def main() -> None:
    scenario = sys.argv[1] if len(sys.argv) > 1 else ""
    checks = {"S080": check_s080, "S081": check_s081, "P021": check_p021}
    if scenario not in checks:
        check_error(f"unknown wave scenario {scenario!r}")
    try:
        checks[scenario]()
    except CheckError as exc:
        check_error(str(exc))
    except SystemExit:
        raise
    except Exception as exc:  # A broken fixture or assertion is exit 3, not a product failure.
        check_error(f"{type(exc).__name__}: {exc}")


if __name__ == "__main__":
    main()
