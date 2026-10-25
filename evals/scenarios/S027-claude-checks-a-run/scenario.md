# S027 Claude checks on a run mid-session
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead, in the same repo, wanting to know if a background task is done, stuck or failing before deciding what to do next.

## Starting state
Two tasks: one implementing with recent steps, one stalled.

## Steps
1. `delegate status --json` and `delegate status t001 --json`.

## Passes when
- The output is valid JSON with, per task: id, phase, worker, tier, start time, time of last activity, stalled or not, files changed and the last few steps.
- The fields match what the human view shows for the same moment.
- The delegate skill tells Claude to use it to check on background runs.

## If this fails
Claude has to guess or scrape human-oriented output to answer "is t004 stuck?", and gets it wrong.

## Failure paths
- An unknown task id: a clear error naming the known tasks, non-zero exit.

## Properties
- Same facts as the human view (P009).
