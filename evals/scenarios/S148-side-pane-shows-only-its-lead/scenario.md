# S148 The side pane puts only its own lead's tasks in the sidebar, and clears them when the conversation changes
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer with two leads (Pi and Claude) in one checkout inside Herdr, each with a
`delegate watch --beside` pane, and the sidebar layout from `--sidebar-config` pasted in.

## Starting state
A git repo with tasks created by both leads (t139-style origins), in varied states:
running, checking, failed, stalled, ready to merge, plus more than four for lead A. One
task has a title containing "failed" and a bidi control. A stand-in Herdr socket answers
`pane.get` and records every request.

## Steps
1. Start `delegate watch --lead-pane` for lead A's pane and for lead B's pane. Let them
   run a few intervals.
2. Fail a task through delegate. Then make A's pane report no conversation and then a
   new one (the previous lead exits and another conversation starts in that pane).
3. Make status unreadable for a while, then stop A's side pane with `q`.

## Passes when
- A's pane receives only A's tasks: four `<state> · <id>` rows in attention order, then
  `+N more`. Unused keys are cleared. B's pane receives only B's tasks.
- Values never contain task titles or control/bidi characters and are at most 80 chars.
  The task titled "failed" isn't coloured or listed as failed for its title.
- A failed task moves to the front as `failed · <id>` within one interval.
- Within one interval of the conversation change, every A token is cleared, and the new
  conversation shows none of A's tasks.
- Every report has a TTL and a rising seq.
- Quitting sends a final report clearing all A tokens.
- The side panes send nothing to Herdr but `pane.get` and `pane.report_metadata`.
  Delegate task state and workers are unchanged by watching.

## If this fails
The sidebar shows tasks under the wrong conversation, keeps a previous lead's tasks
after the pane changes hands, or lets task text change what the rows say.

## Failure paths
- Status can't be read: the side pane shows stale with its age. Tokens are cleared
  once that outlasts the TTL, never replaced by an empty or invented list.
- Herdr unreachable: the pane says "lead unavailable", keeps listing tasks by origin and
  reports nothing.

## Properties
- Tokens on a pane always describe only the tasks whose origin equals that pane's current
  conversation, or are empty.
