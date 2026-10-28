"""S082's text checks: the kit tells the lead to run a wave's tasks together.

Reads the installed `PLAN.md`, `CLAUDE.md` and delegate skill, and (for an upgraded project)
what `delegate upgrade` reported. Matches the instructions the scenario names, loosely enough
that rewording passes, and fails when the skill only teaches `delegate wait <id>`.

For an upgrade, the skill must carry the batch loop and `delegate upgrade` must report the
`CLAUDE.md` wave line and the `PLAN.md` wave column; the check itself compares those two
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


def plan_has_wave_column(text: str) -> bool:
    for line in text.splitlines():
        line = line.lstrip("+- ")
        if not line.startswith("|"):
            continue
        cells = [cell.strip().strip("`*").lower()
                 for cell in line.strip().strip("|").split("|")]
        if "wave" in cells:
            return True
    return False


def claude_names_waves(text: str) -> bool:
    low = flat(text)
    return "wave" in low and any(phrase in low for phrase in (
        "different files", "disjoint", "separate files", "don't overlap", "do not overlap",
        "non-overlapping", "no overlapping files",
    ))


def batch_wait_line(text: str) -> str | None:
    """A `delegate wait` shown without task ids, the way a batch is waited on."""
    for line in text.splitlines():
        low = line.lower()
        if "delegate wait" not in low:
            continue
        after = low.split("delegate wait", 1)[1]
        if re.search(r"<\w+>", after) or re.search(r"\bt\d{3}\b", after):
            continue
        if re.search(r"(--timeout|no ids|no id|every running|all running|all of them)", after) \
                or after.strip() in ("", "`", ".", ",", "and wait again"):
            return line
    return None


def skill_missing(text: str) -> list[str]:
    low = flat(text)
    missing: list[str] = []
    if not ("wave" in low and "run --background" in low
            and re.search(r"\b(ready|every|all)\b", low)):
        missing.append("the skill does not say to start every ready task of a wave with run --background")
    if batch_wait_line(text) is None:
        missing.append("the skill does not show `delegate wait` with no ids as the way to wait on a batch")
    if not re.search(r"act on .{0,80}(whichever|what it reports|first|as each)", low):
        missing.append("the skill does not say to act on whichever task the wait reports")
    if not re.search(r"(while|as) .{0,80}(workers?|they) .{0,80}(run|running|work)", low) \
            or not re.search(r"(next .{0,20}(wave|spec|task)|review)", low):
        missing.append("the skill does not say to write the next specs or review finished tasks while workers run")
    if not ("merge" in low and re.search(r"\bunblock|\bdepend", low)
            and re.search(r"at once|as soon as|immediately|right after|as it lands", low)):
        missing.append("the skill does not say to start a task as soon as the merge it depends on lands")
    if not (("per batch" in low or "one merge question" in low or "one question per batch" in low)
            and "merge" in low and "id" in low):
        missing.append("the skill does not ask one merge question per batch, naming the ids")
    return missing


def previous_kit() -> str:
    """The newest kit commit whose skill does not yet teach the batch loop."""
    sys.path.insert(0, os.environ["EVALS_LIB"])
    import kits  # noqa: E402

    for commit in reversed(kits.commits()):
        files = kits.kit(commit)
        skill = files.get(SKILL, b"").decode("utf-8", errors="replace")
        if skill and skill_missing(skill):
            print(commit)
            return commit
    raise SystemExit("CHECK ERROR: no earlier kit without the batch-loop text was found")


def upgrade_missing(output: str) -> list[str]:
    """The template changes `delegate upgrade` must report for the user to take in."""
    missing: list[str] = []
    low = flat(output)
    if "plan.md" not in low or not plan_has_wave_column(output):
        missing.append("delegate upgrade did not report the `wave` column for PLAN.md")
    if "claude.md" not in low or not claude_names_waves(output):
        missing.append("delegate upgrade did not report the wave line for CLAUDE.md")
    return missing


def main() -> None:
    args = sys.argv[1:]
    if args and args[0] == "previous-kit":
        previous_kit()
        return
    root = Path(args[0])
    upgrade_output = ""
    upgraded = "--upgrade-output" in args
    if upgraded:
        upgrade_output = read(Path(args[args.index("--upgrade-output") + 1]))

    plan = read(root / PLAN)
    claude = read(root / CLAUDE)
    skill = read(root / SKILL)
    missing: list[str] = []
    if upgraded:
        missing.extend(upgrade_missing(upgrade_output))
    else:
        if not plan_has_wave_column(plan):
            missing.append("PLAN.md has no `wave` column")
        if not claude_names_waves(claude):
            missing.append("CLAUDE.md does not say plans are split into waves of tasks that touch different files")
    missing.extend(skill_missing(skill))
    if missing:
        print("; ".join(missing), flush=True)
        raise SystemExit(1)
    print("the kit teaches the wave batch loop", flush=True)


if __name__ == "__main__":
    main()
