"""Skills-layout helpers for the decision 034 checks (S134-S136).

Never hard-codes what the kit ships beyond the contract: the skill names and the
exact symlink target. Every content expectation comes from a fresh init the check
built, and the kit before 034 is found in history like blocks.py does for blocks.
"""

from __future__ import annotations

import os
import sys

SKILLS = ["delegate", "distill", "evaluate"]


def link_target(name: str) -> str:
    """The exact target `.claude/skills/<name>` must be a symlink to."""
    return f"../../.agents/skills/{name}"


def kit_before_034() -> str:
    """Newest kits.py commit whose files have no `.agents/skills/` path.

    Callers first fail as a product failure when the current `delegate init` has
    no `.agents/skills/` layout (t120 not built); against that, the newest
    commit without the layout is the last kit before decision 034.
    """
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import kits  # noqa: E402

    for commit in reversed(kits.commits()):
        if not any(p.startswith(".agents/skills/") for p in kits.kit(commit)):
            return commit
    raise SystemExit("CHECK ERROR: every kit in history already has .agents/skills/")


def main() -> None:
    cmd = sys.argv[1]
    if cmd == "kit-before-034":
        print(kit_before_034())
    elif cmd == "link-target":
        print(link_target(sys.argv[2]))
    elif cmd == "skills":
        print("\n".join(SKILLS))
    else:
        raise SystemExit(f"unknown command {cmd}")


if __name__ == "__main__":
    main()
