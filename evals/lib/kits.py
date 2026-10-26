"""Earlier kits, rebuilt from this repo's git history, for the upgrade checks.

A kit is what `delegate init` installed at a commit: the files listed in that commit's
`delegate/src/init.rs`, with that commit's `delegate/templates/project/` contents. Rebuilding
from history gives the realistic starting state "a project set up by an older kit" without
building old binaries. If init stops listing its files as `include_str!` lines, teach
`kit()` the new way, or later kits drop out of `commits`.

    python3 kits.py commits                 kit commits, oldest first, one per line
    python3 kits.py install <commit> <dir>  write that kit's files into <dir> (a git repo)
    python3 kits.py shipped                 "<path> <sha256>" for every file any kit shipped
    python3 kits.py owned                   project-owned kit paths, one per line

Kit files the project owns: `upgrade` never changes them, it only shows their template's
changes. Every other installed path belongs to the kit.
"""

import hashlib
import os
import re
import subprocess
import sys

ROOT = os.environ["EVALS_ROOT"]
TEMPLATES = "delegate/templates/project/"
PROJECT_OWNED = ["AGENTS.md", "CLAUDE.md", "PLAN.md", ".delegate.toml", "evals/jobs.md"]
EXECUTABLE = ["evals/run"]


def git(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, check=True).stdout


def commits():
    """Commits that changed what init installs, oldest first."""
    out = git("log", "--reverse", "--format=%H", "--", TEMPLATES, "delegate/src/init.rs")
    seen, kits = set(), []
    for c in out.decode().split():
        files = kit(c)
        if not files:
            continue
        key = tuple(sorted((p, hashlib.sha256(b).hexdigest()) for p, b in files.items()))
        if key not in seen:
            seen.add(key)
            kits.append(c)
    return kits


def kit(commit):
    """{path: bytes} that init installed at the commit."""
    try:
        init = git("show", f"{commit}:delegate/src/init.rs").decode()
    except subprocess.CalledProcessError:
        return {}
    paths = re.findall(r'include_str!\("\.\./templates/project/([^"]+)"\)', init)
    return {p: git("show", f"{commit}:{TEMPLATES}{p}") for p in paths}


def install(commit, dest):
    for p, body in kit(commit).items():
        path = os.path.join(dest, p)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(body)
        if p in EXECUTABLE:
            os.chmod(path, 0o755)


def mess_up(rng, proj, commit):
    """Do what a team does to an installed kit: edit, delete and leave uncommitted, at random,
    to kit files and project-owned ones. Returns {path: action}; untouched paths are left out."""
    def sh(*a):
        subprocess.run(a, cwd=proj, capture_output=True, check=True)

    acts = {}
    for p in sorted(kit(commit)):
        a = rng.choice(["keep"] * 3 + ["edit", "edit-uncommitted", "delete", "delete-uncommitted"])
        if a == "keep":
            continue
        acts[p] = a
        path = os.path.join(proj, p)
        if a.startswith("edit"):
            with open(path, "a") as f:
                f.write(rng.choice(["\n## Ours\nRun the slow suite.\n", "\n# local tweak\n", " "]))
        else:
            os.remove(path)
        if not a.endswith("uncommitted"):
            sh("git", "add", "-A", "--", p)
    sh("git", "commit", "-qm", "Team changes", "--allow-empty")
    return acts


def main():
    cmd = sys.argv[1]
    if cmd == "commits":
        print("\n".join(commits()))
    elif cmd == "install":
        install(sys.argv[2], sys.argv[3])
    elif cmd == "shipped":
        lines = {f"{p} {hashlib.sha256(b).hexdigest()}" for c in commits() for p, b in kit(c).items()}
        print("\n".join(sorted(lines)))
    elif cmd == "owned":
        print("\n".join(PROJECT_OWNED))
    else:
        sys.exit(f"unknown command {cmd}")


if __name__ == "__main__":
    main()
