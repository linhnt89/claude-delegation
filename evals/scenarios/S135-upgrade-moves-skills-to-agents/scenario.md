# S135 Upgrade moves an older project's skills to `.agents/skills/`
- Job: J1
- Tier: full
- Judged by: auto
- Status: automated

## Who
A developer in a project set up with the last kit before decision 034, whose skills are real
folders under `.claude/skills/`, who has just rebuilt `delegate` from the current kit.

## Starting state
That project, unchanged kit skills, plus a skill of its own in `.claude/skills/team-notes/`.
All committed.

## Steps
1. `delegate upgrade`.
2. `delegate upgrade` again.

## Passes when
- Each kit skill now lives in `.agents/skills/<name>/` with the current kit's text, and
  `.claude/skills/<name>` is the relative symlink to it.
- `.claude/skills/team-notes/` and `.claude/skills/delegate-pane/` are untouched by the move
  (the pane updates as any kit file does).
- The output names each moved skill and ends with how to review and commit.
- The second upgrade changes nothing.

## If this fails
Projects set up before the change keep skills only Claude Code can find, so leading from Pi
or Codex silently lacks them.

## Failure paths
- See S136 for an edited skill.
