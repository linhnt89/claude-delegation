# S108 The kit teaches pinned contracts, small check tasks and `fix-check`
- Job: J9
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A lead in a project set up with `delegate init`. Decision 025.

## Starting state
A fresh `delegate init` project, and this repo's root kit files.

## Contract
In both the root files and `delegate/templates/project/`:
- `/evaluate` asks each agreed scenario to name what its check observes (command, output or
  file, exit codes).
- The delegate skill says a check task covers at most 5 scenarios and carries the checklist
  (own data, no repo-history assumptions, deterministic order, nothing from the ambient
  environment); teaches the question-phase check review; and teaches `fix-check`, also for
  autopilot.
- The REPORT template has a `## Check problems` section.

## Passes when
- A lead following the kit writes checks that rarely need fixing and fixes them with one command.

## If this fails
The tooling exists but leads keep writing big check batches and hand-written fix specs.
