# S134 A new project's skills are found by any lead harness without flags
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer who starts lead sessions by hand with the harness's plain command (`claude`,
`pi`, `codex`, `opencode`), in a repo where they just ran `delegate init`. Decision 034.

## Starting state
A small git repo with code; `delegate init` has run and its files are committed.

## Steps
1. Look at the installed skills.
2. Clone the repo elsewhere and look again.

## Passes when
- `.agents/skills/delegate/`, `distill/` and `evaluate/` hold the kit's skills, each with a
  `SKILL.md`.
- `.claude/skills/delegate`, `distill` and `evaluate` are relative symlinks to
  `../../.agents/skills/<name>`, and reading `SKILL.md` through them gives the same bytes.
- Git records them as symlinks, and in the clone they still resolve.
- `.claude/skills/delegate-pane/` is still a real folder with the pane mod.

## If this fails
A lead started from Pi or Codex has no `/delegate`, `/distill` or `/evaluate`, or a Claude
Code lead loses them; the user finds out mid-session.

## Failure paths
- `delegate init` in a project that already has its own `.claude/skills/<other>/` leaves it
  alone.
