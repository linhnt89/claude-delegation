# S038 Bring a project set up with an older kit up to the new one
- Job: J1
- Tier: full
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a project set up with an older kit, who has just rebuilt `delegate` from a newer kit.

## Starting state
A project set up by an older kit and used for weeks: filled-in `AGENTS.md`, `PLAN.md`,
`.delegate.toml`, decisions and scenarios of its own. Nobody has edited the kit's skills,
pane mod or `evals/run`. The new kit changed two skills, added a file and dropped one.

## Steps
1. `delegate upgrade`.

## Passes when
- Every kit file the user never touched now matches the new kit; the new file is added, the dropped one removed.
- Project-owned files (`AGENTS.md`, `CLAUDE.md`, `PLAN.md`, `.delegate.toml`, `evals/jobs.md`, decisions, scenarios) are byte-for-byte unchanged.
- The output lists what was updated, added, removed and kept, and ends with "review with git diff, then commit".
- Nothing is committed for them.

## If this fails
Projects stay on old skills and templates after the kit improves, or an upgrade wipes the
project's own plan and rules.

## Failure paths
- Run outside a git repo, or in a repo that was never set up: says so and points to `delegate init`.

## Properties
- Upgrade never changes a project-owned file (P012).
- A second upgrade changes nothing (P013).
- An upgraded project's kit files equal a fresh `init`'s (P014).
