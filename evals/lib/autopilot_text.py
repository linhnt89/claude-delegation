"""S091's text checks: the kit teaches the prep pass, the mandate, parking and handoffs.

Reads the installed `PLAN.md`, `CLAUDE.md` and delegate skill, and (for an upgraded project)
what `delegate upgrade` reported. Matches the instructions the scenario names, loosely enough
that rewording passes. For an upgrade the skill must carry the new text and `delegate upgrade`
must report the `CLAUDE.md` and `PLAN.md` template changes; the check itself compares those
project-owned files byte-for-byte, since upgrade must not edit them (decision 005).
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

SKILL = ".claude/skills/delegate/SKILL.md"
PLAN = "PLAN.md"
CLAUDE = "CLAUDE.md"


def flat(text: str) -> str:
    return re.sub(r"\s+", " ", text.lower())


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def plan_missing(text: str) -> list[str]:
    low = flat(text)
    missing: list[str] = []
    if "## mandate" not in low:
        missing.append("PLAN.md has no '## Mandate' template section")
    for item in ("goal", "gate", "merge"):
        if item not in low:
            missing.append(f"the mandate template does not name '{item}'")
    if not any(phrase in low for phrase in (
        "needs the user", "needs you", "what needs you", "your call",
    )):
        missing.append("the mandate template does not say what needs the user")
    if "## waiting for you" not in low:
        missing.append("PLAN.md has no '## Waiting for you' section")
    return missing


def claude_missing(text: str) -> list[str]:
    low = flat(text)
    missing: list[str] = []
    if "autopilot" not in low:
        missing.append("CLAUDE.md does not mention autopilot")
    if "mandate" not in low:
        missing.append("CLAUDE.md does not mention the plan's mandate")
    if not re.search(r"(handoff|hand off|catch-?up|brief|/clear|new session)", low):
        missing.append("CLAUDE.md does not teach session handoffs or a catch-up brief")
    return missing


def skill_missing(text: str) -> list[str]:
    low = flat(text)
    missing: list[str] = []
    if "mandate" not in low or "autopilot" not in low:
        missing.append("the skill does not teach the prep pass and the mandate before autopilot")
    if not all(word in low for word in ("goal", "gate", "merge")):
        missing.append("the mandate is not described with its goal, gate and merge rule")
    if not any(phrase in low for phrase in (
        "needs the user", "needs you", "what needs you", "your call", "waiting for you",
    )):
        missing.append("the mandate does not say what still needs the user")
    if "waiting for you" not in low or not any(word in low for word in ("park", "question")):
        missing.append("the skill does not teach parking a task under 'Waiting for you'")
    if "handoff" not in low or "plan.md" not in low:
        missing.append("the skill does not teach writing a handoff in PLAN.md at the end of a phase")
    if "session" not in low or not ("/clear" in low or "new session" in low):
        missing.append("the skill does not teach one session per phase with /clear or a new session")
    if not any(word in low for word in ("catch-up", "catch up", "brief")) or "session" not in low:
        missing.append("the skill does not teach a catch-up brief at session start")
    if not re.search(r"autopilot.{0,160}lock|lock.{0,160}autopilot", low):
        missing.append("the skill does not teach leaving tasks alone while the autopilot lock is held")
    if not re.search(r"(don't|do not|never|leave).{0,120}(run|merge|discard)", low):
        missing.append("the skill does not say to leave run, merge and discard alone while autopilot runs")
    return missing


def upgrade_missing(output: str) -> list[str]:
    low = flat(output)
    missing: list[str] = []
    if "plan.md" not in low or not any(word in low for word in ("mandate", "waiting for you")):
        missing.append("delegate upgrade did not report the PLAN.md mandate and waiting-section changes")
    if "claude.md" not in low or not any(
        word in low for word in ("autopilot", "session", "handoff", "catch-up", "catch up")
    ):
        missing.append("delegate upgrade did not report the CLAUDE.md autopilot and handoff changes")
    return missing


def previous_kit() -> str:
    """The newest kit whose skill does not yet teach the autopilot text."""
    sys.path.insert(0, os.environ["EVALS_LIB"])
    import kits  # noqa: E402

    for commit in reversed(kits.commits()):
        files = kits.kit(commit)
        skill = files.get(SKILL, b"").decode("utf-8", errors="replace")
        if skill and skill_missing(skill):
            print(commit)
            return commit
    raise SystemExit("CHECK ERROR: no earlier kit without the autopilot text was found")


def main() -> None:
    args = sys.argv[1:]
    if args and args[0] == "--previous-kit":
        previous_kit()
        return
    root = Path(args[0])
    upgraded = "--upgrade-output" in args
    upgrade_output = ""
    if upgraded:
        upgrade_output = read(Path(args[args.index("--upgrade-output") + 1]))
    plan = read(root / PLAN)
    claude = read(root / CLAUDE)
    skill = read(root / SKILL)
    missing: list[str] = []
    if upgraded:
        missing.extend(upgrade_missing(upgrade_output))
    else:
        missing.extend(plan_missing(plan))
        missing.extend(claude_missing(claude))
    missing.extend(skill_missing(skill))
    if missing:
        print("; ".join(missing), flush=True)
        raise SystemExit(1)
    print("the kit teaches autopilot, the mandate, parking and session handoffs", flush=True)


if __name__ == "__main__":
    main()
