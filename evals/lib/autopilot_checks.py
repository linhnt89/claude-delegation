"""Property checks P022-P024: autopilot's checkout boundary, one report/notify per run, and
one lead at a time. Driven through the real CLI with the stand-in lead and workers from
common.sh; the seed is printed and a failing sequence is shrunk before it is reported.

    evals/lib/autopilot_checks.py P022|P023|P024
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import random
import signal
import subprocess
import sys
import time
from typing import Callable

sys.path.insert(0, os.environ["EVALS_LIB"])
import kit  # noqa: E402
from kit import DELEGATE, HOME, PROJ, W, d, git, seed, shrink, write  # noqa: E402

AP = Path(W) / "autopilot"
CLAUDE = Path(W) / "bin/claude"
NOTIFY = Path(W) / "bin/record-notify"
RELEASE = "t001-release"

WORKERS = """planner_family = "claude"
[defaults]
implement = "standard"
research = "fast"
review = "standard"
plan_review = "standard"
[implement]
fast = ["pi:gpt-good@high"]
standard = ["pi:gpt-good@high"]
deep = ["pi:gpt-good@high", "claude:sonnet"]
[research]
fast = ["pi:deepseek-research@medium"]
[review]
standard = ["pi:gpt-review@high", "pi:deepseek-review@high"]
[plan_review]
standard = ["pi:claude-plan@high", "pi:deepseek-plan@high"]
"""

WORKER = r'''
task_id() { sed -n '1s/^# Task \([^:]*\):.*/\1/p' .task/SPEC.md 2>/dev/null; }
mode() { cat "$EVALS_WORK/flags/$1" 2>/dev/null || echo pass; }
ask() {
  id="$(task_id)"; m="$(mode "$id")"
  if [ "$m" = questions ]; then
    printf 'QUESTIONS\n1. Should %s round down or to the nearest cent?\n' "$id" >.task/QUESTIONS.md
  else
    printf 'READY\n' >.task/QUESTIONS.md
  fi
}
work() {
  id="$(task_id)"; m="$(mode "$id")"
  case "$m" in
    questions) : ;;
    crash) exit 1 ;;
    fail) printf '# %s wrong\n' "$id" >"shop/${id}.py" ;;
    broken) printf '# %s ok\n' "$id" >"shop/${id}.py"; printf 'broken\n' >shop/broken.txt ;;
    slow) mkdir -p "$EVALS_WORK/flags"; printf '%s\n' "$id" >"$EVALS_WORK/flags/$id-started"; wait_flag "$id-release" 300 || true; printf '# %s ok\n' "$id" >"shop/${id}.py" ;;
    *) printf '# %s ok\n' "$id" >"shop/${id}.py" ;;
  esac
}
'''


class ProductFail(Exception):
    pass


def install_lead() -> None:
    AP.mkdir(parents=True, exist_ok=True)
    CLAUDE.write_text(
        "#!/usr/bin/env bash\n"
        f'export EVALS_WORK="{W}"\nexport EVALS_LIB="{os.environ["EVALS_LIB"]}"\n'
        'printf "%s\\n" "${DELEGATE_AUTOPILOT_WAVE:-0}" >>"$EVALS_WORK/autopilot/waves"\n'
        'printf "%s\\n" "${DELEGATE_AUTOPILOT:-}" >"$EVALS_WORK/autopilot/run-id"\n'
        'printf "%s\\n" "$$" >"$EVALS_WORK/autopilot/lead.pid"\n'
        '. "$EVALS_LIB/autopilot_lead.sh"\n'
        '. "$EVALS_WORK/autopilot/lead.sh"\n'
        'if declare -F wave >/dev/null; then wave || true; fi\n'
        'printf \'{"type":"result","subtype":"success","is_error":false,"result":"ok"}\\n\'\n',
        encoding="utf-8",
    )
    CLAUDE.chmod(0o755)
    NOTIFY.write_text(
        "#!/usr/bin/env bash\n"
        '{ printf "MESSAGE=%s\\n" "$DELEGATE_MESSAGE"\n'
        '  printf "TITLE=%s\\n" "$DELEGATE_TITLE"\n'
        '  printf "PROJECT=%s\\n" "$DELEGATE_PROJECT"\n'
        '  printf "REPORT=%s\\n" "$DELEGATE_REPORT"\n'
        f'}} >>"{W}/notify.log"\n',
        encoding="utf-8",
    )
    NOTIFY.chmod(0o755)


def workers(notify: str | None) -> None:
    text = WORKERS
    if notify:
        text += f'\n[autopilot]\nnotify = "{notify}"\n'
    kit.workers(text)


def worker() -> None:
    write(str(Path(W) / "stub/gpt-good"), WORKER)


def lead(wave_size: int, ids: list[str], deps: dict[str, str] | None = None) -> None:
    deps = deps or {}
    pairs = " ".join(f"{key}={value}" for key, value in deps.items())
    write(str(AP / "lead.sh"),
          f'LEAD_WAVE_SIZE={wave_size}\nLEAD_ORDER="{" ".join(ids)}"\nLEAD_DEPS="{pairs}"\n')


def plan(gate: str, ids: list[str]) -> None:
    lines = "\n".join(f"| 1 | {tid} | do {tid} | ready | – |" for tid in ids)
    write(str(Path(PROJ) / "PLAN.md"),
          "# Plan\n\n## Mandate\n"
          f"- Goal: {', '.join(ids)}\n- Gate: `{gate}`\n"
          "- Merge without asking when: the checks pass\n"
          "- Needs the user: new decisions\n\n"
          "## Tasks\n| wave | id | task | status | depends on |\n"
          "|------|----|------|--------|------------|\n" + lines + "\n")
    subprocess.run(["git", "add", "PLAN.md"], cwd=PROJ, check=True, capture_output=True)
    subprocess.run(["git", "commit", "-qm", "Plan with the autopilot mandate"], cwd=PROJ,
                   check=True, capture_output=True)


def specs(ids: list[str], acceptance: str | None = None) -> None:
    for tid in ids:
        rc, out = d("new", tid)
        if rc != 0:
            raise ProductFail(f"delegate new {tid}: {out}")
        path = out.strip().splitlines()[-1].strip()
        cmd = acceptance or f"grep -qx '# {tid} ok' shop/{tid}.py"
        write(path, f"# Task {tid}: Do {tid}\n\n## Goal\nMake {tid} pass.\n\n"
                    "## Scope\n- In: `shop/**`\n\n"
                    f"## Acceptance\n- [ ] `{cmd}`\n")


def status() -> dict:
    try:
        p = subprocess.run([DELEGATE, "autopilot", "status", "--json"], cwd=PROJ,
                           capture_output=True, text=True)
        value = json.loads(p.stdout)
        return value if isinstance(value, dict) else {}
    except Exception:
        return {}


def start(*args: str) -> None:
    rc, out = d("autopilot", "start", *args)
    if rc != 0:
        raise ProductFail(f"delegate autopilot start {' '.join(args)}: {out}")


def wait_stopped(seconds: int = 90) -> dict:
    end = time.time() + seconds
    while time.time() < end:
        state = status()
        if state.get("running") is False and state.get("stop_reason"):
            return state
        time.sleep(0.2)
    return status()


def wait_started(tid: str = "t001", seconds: int = 30) -> bool:
    end = time.time() + seconds
    while time.time() < end:
        state = status()
        if (Path(W) / "flags" / f"{tid}-started").exists() or state.get("running") \
                or state.get("stop_reason"):
            return True
        time.sleep(0.2)
    return False


def merged() -> set[str]:
    out = subprocess.run(["git", "log", "--format=%s", "autopilot"], cwd=PROJ,
                         capture_output=True, text=True).stdout
    return {line.split(":", 1)[0] for line in out.splitlines() if line[:1] == "t" and ": " in line}


def snapshot() -> str:
    base = git("rev-parse", "--abbrev-ref", "HEAD")
    files = subprocess.run(
        ["bash", "-c", "find . -path ./.git -prune -o -type f -print0 | sort -z | xargs -0 sha256sum"],
        cwd=PROJ, capture_output=True, text=True).stdout
    ref = git("rev-parse", f"refs/heads/{base}")
    index = git("status", "--porcelain")
    return f"{files}\nBASE {ref}\n{index}"


def same(before: str, what: str) -> None:
    now = snapshot()
    if now != before:
        diff = [line for line in now.splitlines() if line not in before.splitlines()][:3]
        raise ProductFail(f"{what}: the user's checkout or base branch changed ({diff})")


def reset(modes: dict[str, str], ids: list[str], wave_size: int = 1,
          gate: str = "true") -> str:
    kit.fresh()
    for path in (AP, Path(W) / "notify.log"):
        if path.is_dir():
            subprocess.run(["rm", "-rf", str(path)], check=False)
        elif path.exists():
            path.unlink()
    AP.mkdir(parents=True, exist_ok=True)
    for tid, mode in modes.items():
        kit.flag(tid, mode)
    worker()
    workers(str(NOTIFY))
    specs(ids)
    lead(wave_size, ids)
    plan(gate, ids)
    return snapshot()


def release(ids: list[str] | None = None) -> None:
    for tid in ids or ["t001"]:
        kit.flag(f"{tid}-release", "go")


def cleanup_run() -> None:
    state = status()
    pid = state.get("pid")
    if isinstance(pid, int):
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    subprocess.run(["pkill", "-KILL", "-f", str(Path(W) / "bin/")], check=False)


def report_files() -> list[Path]:
    state = status()
    log = state.get("log")
    if not log:
        return []
    return sorted(Path(log).glob("*.md"))


def notify_count() -> int:
    path = Path(W) / "notify.log"
    if not path.exists():
        return 0
    return sum(1 for line in path.read_text(encoding="utf-8").splitlines() if line.startswith("MESSAGE="))


def case_fail(message: str, s: int, smallest: object = None) -> None:
    if smallest is not None:
        print(f"smallest failing sequence: {smallest}")
    kit.fail(message, s=s)


# --- P022: the checkout changes only through land -------------------------------------------------

def p022(rng: random.Random, s: int) -> None:
    ok = ["pass", "fail", "question", "crash", "slow"]
    sequences = [
        ["start"],
        ["start", "stop", "start"],
        ["start", "kill", "start"],
        ["start", "start"],
        ["start", "stop"],
    ]

    def trial(seq: list[str], modes: dict[str, str]) -> str | None:
        before = reset(modes, ["t001", "t002"])
        started = False
        try:
            for action in seq:
                if action == "start":
                    if status().get("running"):
                        rc, out = d("autopilot", "start")
                        if rc == 0:
                            return "a second autopilot start was allowed while one was running"
                    else:
                        start()
                        started = True
                        wait_started("t001")
                elif action == "stop":
                    if status().get("running"):
                        d("autopilot", "stop")
                    wait_stopped(30)
                elif action == "kill":
                    state = status()
                    pid = state.get("pid")
                    if isinstance(pid, int):
                        os.kill(pid, signal.SIGKILL)
                    wait_stopped(30)
                same(before, f"the checkout changed after '{action}'")
            if started:
                release(["t001", "t002"])
                wait_stopped(60)
            same(before, "the checkout changed before land")
            return None
        except ProductFail as error:
            return str(error)
        finally:
            release(["t001", "t002"])
            cleanup_run()

    failing = None
    failing_modes: dict[str, str] = {}
    for seq in sequences:
        modes = {"t001": rng.choice(ok), "t002": rng.choice(ok)}
        problem = trial(seq, modes)
        if problem:
            failing = seq
            failing_modes = modes
            break
    if failing is None:
        print(f"seed {s}: every autopilot start/stop/kill/restart sequence left the checkout alone")
        return
    smallest = shrink(failing, lambda seq: trial(seq, failing_modes) is not None)
    case_fail(trial(smallest, failing_modes) or "the checkout changed", s, smallest)


# --- P023: one notification and one report per run ------------------------------------------------

def p023(rng: random.Random, s: int) -> None:
    from quota_checks import claude_body, make_claude_login, route  # noqa: PLC0415
    from vendors import VendorServer  # noqa: PLC0415

    reasons = ["goal_met", "waiting_for_user", "gate_failed", "wave_limit", "stopped", "quota_floor"]
    rng.shuffle(reasons)
    for reason in reasons:
        size = 3 if reason == "wave_limit" else rng.randint(1, 3)
        ids = [f"t{i:03d}" for i in range(1, size + 1)]
        gate = "true"
        modes = {tid: "pass" for tid in ids}
        args = ()
        server = None
        extra = ""
        if reason == "waiting_for_user":
            modes[ids[0]] = "questions"
        elif reason == "gate_failed":
            modes[ids[-1]] = "broken"
            gate = "test ! -e shop/broken.txt"
        elif reason == "wave_limit":
            args = ("--max-waves", "1")
        elif reason == "stopped":
            modes[ids[0]] = "slow"
        elif reason == "quota_floor":
            make_claude_login("FAKE_T064_P023_CLAUDE")
            server = VendorServer()
            server.set_routes({"GET /api/oauth/usage": route(body=claude_body(95, 95, 7200, 7200))})
            os.environ.update(server.env())
            args = ("--quota-floor", "5h")
        try:
            before = reset(modes, ids, wave_size=1, gate=gate)
            if reason == "stopped":
                start(*args)
                wait_started(ids[0])
                d("autopilot", "stop")
            else:
                start(*args)
            state = wait_stopped(90)
            if not state.get("stop_reason"):
                case_fail(f"autopilot did not stop for {reason} (status: {state})", s)
            if state.get("stop_reason") != reason:
                case_fail(f"stop reason is {state.get('stop_reason')!r}, want {reason!r}", s)
            if status().get("running") is not False:
                case_fail(f"autopilot still reads as running after {reason}", s)
            if notify_count() != 1:
                case_fail(f"the {reason} run made {notify_count()} notify attempts, want 1", s)
            for _ in range(25):
                if report_files():
                    break
                time.sleep(0.2)
            for _ in range(25):
                if notify_count():
                    break
                time.sleep(0.2)
            reports = report_files()
            if len(reports) != 1:
                case_fail(f"the {reason} run wrote {len(reports)} reports, want 1", s)
            text = reports[0].read_text(encoding="utf-8", errors="replace").lower()
            wanted = reason.replace("_", " ").split()
            if not all(word[:4] in text for word in wanted):
                case_fail(f"the {reason} run's report does not name that reason: {text[:200]}", s)
            same(before, f"the {reason} run changed the user's checkout")
            extra = reason
        finally:
            release(ids)
            cleanup_run()
            if server is not None:
                server.close()
                for key in ("DELEGATE_QUOTA_BASE_CLAUDE", "DELEGATE_QUOTA_BASE_CODEX",
                            "DELEGATE_QUOTA_BASE_COPILOT", "DELEGATE_QUOTA_BASE_HYPER"):
                    os.environ.pop(key, None)
                (Path(HOME) / ".claude/.credentials.json").unlink(missing_ok=True)
    print(f"seed {s}: {len(reasons)} stop reasons each made one notification and one report ({extra})")


# --- P024: one lead at a time ---------------------------------------------------------------------

def p024(rng: random.Random, s: int) -> None:
    actions = ["run t001", "run t002", "run t005", "merge t001", "merge t002",
               "discard t001", "discard t002"]

    def d_env(extra: dict[str, str], *args: str) -> tuple[int, str]:
        env = os.environ.copy()
        env.pop("DELEGATE_AUTOPILOT", None)
        env.update(extra)
        p = subprocess.run([DELEGATE, *args], cwd=PROJ, capture_output=True,
                           text=True, env=env)
        out = p.stdout + p.stderr
        with open(os.path.join(W, "transcript.log"), "a") as f:
            f.write(f"\n$ DELEGATE_AUTOPILOT={extra.get('DELEGATE_AUTOPILOT', '-')} "
                    f"delegate {' '.join(args)}  (exit {p.returncode})\n{out}")
        return p.returncode, out

    def read(name: str) -> str:
        try:
            return (AP / name).read_text(encoding="utf-8").strip()
        except OSError:
            return ""

    def alive(pid: int) -> bool:
        try:
            os.kill(pid, 0)
            return True
        except (ProcessLookupError, PermissionError):
            return False

    def refused(extra: dict[str, str], action: str, when: str) -> str | None:
        rc, out = d_env(extra, *action.split())
        if rc == 0:
            return f"delegate {action} was allowed {when}"
        if "delegate autopilot stop" not in out:
            return f"delegate {action} was refused {when} without naming 'delegate autopilot stop': {out}"
        return None

    def trial(seq: list[str]) -> str | None:
        before = reset({"t001": "slow", "t002": "pass"}, ["t001", "t002"])
        specs(["t005"])
        try:
            start()
            if not wait_started("t001", 30):
                return "autopilot never started wave 1"
            for action in seq:
                problem = refused({}, action, "while autopilot held the lock")
                if problem:
                    return problem
                same(before, f"delegate {action} changed the checkout while autopilot ran")
            for action in ("run t002", "merge t002", "discard t002"):
                problem = refused({"DELEGATE_AUTOPILOT": "not-the-live-run"}, action,
                                  "while autopilot held the lock with a wrong run id")
                if problem:
                    return problem
            run_id = read("run-id")
            if not run_id:
                return "the lead was not given DELEGATE_AUTOPILOT=<run id>"
            lead_pid = read("lead.pid")
            if not lead_pid.isdigit():
                return "the stand-in lead did not record its process id"
            state = status()
            pid = state.get("pid")
            if not isinstance(pid, int):
                return "status named no driver pid to kill"
            os.kill(pid, signal.SIGKILL)
            state = wait_stopped(30)
            if state.get("running") is not False:
                return "autopilot still reads as running after the driver was killed"
            if not alive(int(lead_pid)):
                return "the lead of the killed run was gone before the restart"
            for action in ("run t002", "merge t001"):
                problem = refused({"DELEGATE_AUTOPILOT": run_id}, action,
                                  "after its driver died, holding the dead run id")
                if problem:
                    return problem
            start()
            end = time.time() + 30
            while alive(int(lead_pid)) and time.time() < end:
                time.sleep(0.1)
            if alive(int(lead_pid)):
                return "a new start left the killed run's lead process alive"
            release(["t001"])
            state = wait_stopped(90)
            if state.get("running") is not False:
                return "the restarted run did not stop"
            rc, out = d("run", "t005", "--no-ask")
            if rc != 0:
                return f"after the driver ended, delegate run t005 was still locked out: {out}"
            return None
        except ProductFail as error:
            return str(error)
        finally:
            release(["t001"])
            cleanup_run()

    failing = None
    for _ in range(3):
        seq = rng.sample(actions, rng.randint(1, 3))
        problem = trial(seq)
        if problem:
            failing = seq
            break
    if failing is None:
        print(f"seed {s}: every run/merge/discard interleaving was refused while autopilot held work, "
              "and a dead driver left no lead process or usable run id")
        return
    smallest = shrink(failing, lambda seq: trial(seq) is not None)
    case_fail(trial(smallest) or "a second lead got through", s, smallest)

def main() -> None:
    which = sys.argv[1]
    install_lead()
    kit.save_template()
    rng, s = seed()
    try:
        {"P022": p022, "P023": p023, "P024": p024}[which](rng, s)
    except ProductFail as error:
        kit.fail(str(error), s=s)
    except Exception as error:  # a broken fixture is a check error, not a product verdict
        cleanup_run()
        print(f"CHECK ERROR: {type(error).__name__}: {error}")
        raise SystemExit(3) from error


if __name__ == "__main__":
    main()
