# S097 A hidden check's writers are kept away from the work it judges
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, whose hidden set includes a check for the job a task works on. Decision 023.

## Starting state
t001 wrote the checks for S001 and S002 (job J1) and was merged with a GPT-family trailer.
The lead then hid S002 with `delegate hidden add`. t003's spec has `evals/run S001` in its
Acceptance; t003 works on J1. The implement tier lists a GPT-family model first and a
DeepSeek-family model second. A second check task, t004, wrote hidden S003 for J2 with a
Claude-family trailer.

## Steps
1. `delegate run t003`.

## Passes when
- t003 runs on the DeepSeek-family model, and the run says it skipped the GPT family for the
  checks of S001 and a hidden scenario. It names no hidden scenario and nothing from its content.
- The Claude family is not skipped: S003 judges another job.
- The worker still can't read the hidden set.

## If this fails
The checks the user kept hidden to catch tailored work are written by the same kind of model
as the work, so they catch less than the user thinks.

## Failure paths
- The vault is missing or unreadable: the run says hidden checks' writers couldn't be found and
  goes on with the visible ones.
