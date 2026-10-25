"""Helpers for property checks: drive `delegate` the way a user does, many times, seeded.

Use after `. common.sh; setup_project` in the check, which exports PROJ, DHOME, DELEGATE,
HOME and PATH (stand-in `pi` first). `save_template()` keeps that project; `fresh()` resets
to it before each generated case.
"""

import os
import random
import re
import shutil
import subprocess
import sys

W = os.environ["EVALS_WORK"]
PROJ = os.environ["PROJ"]
DHOME = os.environ["DHOME"]
DELEGATE = os.environ["DELEGATE"]
HOME = os.environ["HOME"]
TEMPLATE = os.path.join(W, "template")
CALLS = os.path.join(HOME, ".cache/stub/calls")
TIERS = ["fast", "standard", "deep"]

GOOD = "def share(amount, people):\n    return round(amount / people, 2)\n"
BAD = "def share(amount, people):\n    return amount // people\n"


def seed():
    s = int(os.environ.get("EVALS_SEED") or random.SystemRandom().randrange(1, 10**6))
    print(f"seed {s} (rerun with EVALS_SEED={s})")
    return random.Random(s), s


def sh(cmd, cwd=PROJ):
    return subprocess.run(cmd, cwd=cwd, shell=isinstance(cmd, str), capture_output=True, text=True)


def d(*args, cwd=PROJ):
    """Run delegate; returns (exit code, combined output)."""
    p = subprocess.run([DELEGATE, *args], cwd=cwd, capture_output=True, text=True)
    out = p.stdout + p.stderr
    with open(os.path.join(W, "transcript.log"), "a") as f:
        f.write(f"\n$ delegate {' '.join(args)}  (exit {p.returncode})\n{out}")
    return p.returncode, out


def git(*args, cwd=PROJ):
    return sh(["git", *args], cwd=cwd).stdout.strip()


def save_template():
    shutil.rmtree(TEMPLATE, ignore_errors=True)
    shutil.copytree(PROJ, TEMPLATE, symlinks=True)


def fresh():
    for p in (PROJ, DHOME, CALLS, os.path.join(W, "flags")):
        shutil.rmtree(p, ignore_errors=True)
    shutil.copytree(TEMPLATE, PROJ, symlinks=True)
    os.makedirs(CALLS, exist_ok=True)
    os.makedirs(os.path.join(W, "flags"), exist_ok=True)


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


def flag(name, value):
    write(os.path.join(W, "flags", name), value + "\n")


def stub(model, body):
    write(os.path.join(W, "stub", model), body)


def workers(text):
    write(os.path.join(HOME, ".config/delegate/workers.toml"), text)


def spec(tid, cmds=("python3 -c 'from shop.split import share; assert share(10, 3) == 3.33'",), title="Split a bill"):
    rc, out = d("new", tid)
    if rc != 0:
        raise RuntimeError(f"delegate new {tid}: {out}")
    acc = "\n".join(f"- [ ] `{c}`" for c in cmds)
    write(os.path.join(DHOME, tid, "SPEC.md"),
          f"# Task {tid}: {title}\n\n## Goal\nLet the checkout split a bill.\n\n"
          f"## Scope\n- In: `shop/**`\n\n## Acceptance\n{acc}\n")


# A stand-in whose behaviour per task comes from flags/<task id>: pass | fail | questions | nostart.
# It writes shop/split_<id>.py (good or bad) so tasks never conflict.
FLAGGED = r'''
tid="$(sed -n '1s/^# Task \([^:]*\):.*/\1/p' .task/SPEC.md 2>/dev/null)"
mode="$(cat "$EVALS_WORK/flags/$tid" 2>/dev/null || echo pass)"
start() { [ "$mode" = nostart ] && { echo "Error: 429 quota exceeded" >&2; exit 1; }; :; }
ask() { if [ "$mode" = questions ]; then printf 'QUESTIONS\n1. Round down or to the nearest cent?\n' >.task/QUESTIONS.md; else echo READY >.task/QUESTIONS.md; fi; }
work() {
  f="shop/split_${tid}.py"
  case "$mode" in
    pass) printf '%s' "$GOOD_PY" >"$f" ;;
    fail) printf '%s' "$BAD_PY" >"$f" ;;
  esac
}
'''
os.environ["GOOD_PY"] = GOOD
os.environ["BAD_PY"] = BAD


def task_cmd(tid):
    return f"python3 -c 'from shop.split_{tid} import share; assert share(10, 3) == 3.33'"


def list_tasks():
    """`delegate list` as {id: (round, tier, status, worker)} or {id: 'not run'}."""
    rc, out = d("list")
    tasks = {}
    for line in out.splitlines():
        m = re.match(r"^(\S+)\s+round (\d+)\s+(\S+)\s+(.*?)\s+\[(.*)\]$", line)
        if m:
            tasks[m[1]] = (int(m[2]), m[3], m[4], m[5])
        elif re.match(r"^(\S+)\s+not run$", line):
            tasks[line.split()[0]] = "not run"
    return tasks


def task_state_left(tid):
    """What of a task is still around: folder, worktree, branch."""
    left = []
    if os.path.exists(os.path.join(DHOME, tid)):
        left.append(f"folder {DHOME}/{tid}")
    if git("rev-parse", "-q", "--verify", f"refs/heads/task/{tid}"):
        left.append(f"branch task/{tid}")
    wts = [l for l in git("worktree", "list", "--porcelain").splitlines() if l.startswith("worktree ")]
    if len(wts) > 1:
        left.append(f"worktree {wts[1:]}")
    return left


def shrink(case, still_fails, limit=40):
    """Smallest sub-sequence of `case` that still fails, by dropping one step at a time."""
    tries = 0
    changed = True
    while changed and tries < limit:
        changed = False
        for i in range(len(case)):
            smaller = case[:i] + case[i + 1:]
            tries += 1
            if smaller and still_fails(smaller):
                case, changed = smaller, True
                break
    return case


def fail(msg, case=None, s=None):
    if case is not None:
        print(f"smallest failing case: {case}")
    print(f"FAIL: {msg}" + (f" (seed {s})" if s is not None else ""))
    sys.exit(1)
