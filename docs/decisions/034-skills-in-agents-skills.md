# 034: The kit's skills live in `.agents/skills/`, linked from `.claude/skills/`

- Date: 2026-11-03
- Status: accepted

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
Decision 031 lets any model lead, but the kit's skills (`delegate`, `distill`, `evaluate`) sit
in `.claude/skills/`, which only Claude Code and OpenCode discover. The user starts lead
sessions by hand with the harness's plain command, so a launcher, alias or `--skill` flags
don't count. A probe (2026-11-03) of where each harness discovers project skills:
Claude Code `.claude/skills/` only, following symlinks; Pi and Codex `.agents/skills/` only;
OpenCode both, listing a skill reached through both once. Gemini was not tested (no login).

## Decision
- **One copy, in the standard place.** Each kit skill lives in `.agents/skills/<name>/`
  (Agent Skills location). `.claude/skills/<name>` is a relative symlink to
  `../../.agents/skills/<name>` so Claude Code finds the same files.
- `.claude/skills/delegate-pane/` stays as it is: it is a Claude Code plugin, not a skill.
- **Kit files can be symlinks.** `delegate init` creates the links; `delegate upgrade`
  treats a link as a kit file whose content is its target path: a missing link is added, a
  link the user replaced or changed is kept and reported, like an edited file.
- **Old projects move on upgrade.** A pristine `.claude/skills/<name>/` from an earlier kit is
  replaced by the `.agents/skills/<name>/` copy plus the link. An edited one is kept and
  reported with how to take the kit's layout; nothing is merged.
- **Workers unchanged.** Pi workers run with `--approve` and Codex reads `.agents/skills/`,
  so workers see the lead's skills, as Claude and OpenCode workers already do through
  `.claude/skills/`. The `lead` block's worker check keeps them in their role.
- Pi asks once per project to trust `.agents/skills/` in an interactive session; the README
  says so.

## Rejected options
- A launcher, alias or `pi --skill` flags — the user starts lead sessions by hand.
- Two real copies, `.agents/skills/` and `.claude/skills/`, kept equal by upgrade — they drift
  as soon as one is edited.
- Only `.agents/skills/` — Claude Code doesn't discover it.
- `--no-skills` for Pi workers — drops the user's own skills too, for little gain.

## Consequences
The kit works with Pi, Codex, OpenCode and Claude Code without flags. Init and upgrade gain
symlink support. On Windows without symlink support in git, the links check out as text
files; not supported for now.
