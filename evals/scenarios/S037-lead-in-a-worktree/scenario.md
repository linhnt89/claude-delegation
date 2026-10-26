# S037 Claude leads from a git worktree of the project
- Job: J2
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead, in a session the desktop app started in its own git worktree of the
project (`.claude/worktrees/<name>`, on its own branch), with `delegate` on PATH.

## Starting state
A project with the kit, one user scenario of its own (`evals/scenarios/S001-...`) and hidden
scenarios in the project's delegate home (`<repo>.delegate/_evals/hidden/`). The lead's
worktree sits inside the main checkout, as the desktop app makes it.

## Steps
1. From the lead's worktree: `delegate new t001`, write the spec with `evals/run S001` in
   Acceptance (no `EVALS_RESULTS` or `DELEGATE_HOME` set).
2. `delegate run t001`, `delegate review t001`, `delegate merge t001`, `delegate list`.

## Passes when
- The task's folder is in the project's one delegate home, `<repo>.delegate/` beside the main
  checkout, the same place `evals/run` keeps hidden scenarios and results; no
  `<worktree>.delegate/` folder appears.
- The run's acceptance check `evals/run S001` passes inside the sandbox.
- The worker can't read the hidden scenarios.
- The merge lands on the lead's branch, and `delegate list` then shows no tasks.

## If this fails
A lead working from a worktree (the desktop app's default) can't edit its own specs, can't
use the project's user scenarios as acceptance checks without a workaround, or hands the
worker the hidden checks.

## Failure paths
- None beyond the above.

## Properties
- Every checkout of one repo uses the same delegate home.
