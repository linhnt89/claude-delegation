# S039 Upgrade a project where someone edited a kit file
- Job: J1
- Tier: full
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a project set up with an older kit, who has just rebuilt `delegate` from a newer kit.

## Starting state
As S038, but the owner tuned `.claude/skills/delegate/SKILL.md` for their team, and the new
kit also changed that file.

## Steps
1. `delegate upgrade`.

## Passes when
- Their edited skill is byte-for-byte unchanged; the untouched kit files are updated as in S038.
- The output names the edited file, says the kit has a newer version of it, and says how to
  see the difference and how to take the new version.
- Taking the new version of that one file, as the output says, leaves the rest alone.

## If this fails
An upgrade silently throws away a team's own changes to a skill, and nobody notices until
Claude behaves differently.

## Failure paths
- A kit file with uncommitted edits counts as edited.
- A kit file the user deleted on purpose is not added back silently: the output says it is missing and how to restore it.
