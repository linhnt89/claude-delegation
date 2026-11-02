# S136 Upgrade keeps a skill the project edited where it is
- Job: J1
- Tier: full
- Judged by: auto
- Status: automated

## Who
As S135, but the team tuned `.claude/skills/delegate/SKILL.md`.

## Starting state
As S135, with that one edit committed.

## Steps
1. `delegate upgrade`.
2. The command the output gives to take the kit's layout for that skill.

## Passes when
- Step 1 leaves `.claude/skills/delegate/` byte-for-byte as it was and adds no
  `.agents/skills/delegate/`, so no harness sees two different versions; the other skills
  move as in S135.
- The output says the skill was kept because it was edited, how to compare it with the
  kit's, and how to take the kit's layout.
- Step 2 moves that skill as in S135.

## If this fails
An upgrade throws away a team's tuned skill, or leaves Claude Code and Pi reading different
versions of it.

## Failure paths
- The user replaced a kit symlink with a folder of their own (after the move): kept and
  reported, never overwritten.
- The user deleted a kit symlink: reported missing with how to restore it, not re-added.
