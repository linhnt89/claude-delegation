# S105 The implementer flags a check it can't pass before doing any work
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, starting an implementer whose Acceptance check is wrong. Decision 025.

## Starting state
t003's Acceptance has `evals/run S001`. In the question phase, its worker writes
`.task/QUESTIONS.md` with first line `READY` and a `## Check problems` section naming S001 and a
reason.

## Steps
1. `delegate run t003`.

## Contract
- The run stops after the question phase, as it does for `QUESTIONS`; the work round doesn't start.
- `delegate status t003` (and `--json`) shows it waiting on the lead, with S001 and the worker's
  reason.
- The question-phase prompt asks the worker to read its Acceptance checks against their
  `scenario.md` and report, not fix, any it thinks can't pass.

## Passes when
- A bad check is caught before a round is spent on it.

## If this fails
Check bugs surface only after a failed round, which is where nearly all the cost came from.
