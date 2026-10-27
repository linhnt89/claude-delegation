# S047 A hidden scenario added on one machine is there on the other
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
The developer on two machines, both set up with `delegate setup --vault <url>`.

## Starting state
The same project on both machines, the same hidden set on both.

## Steps
1. On machine A, the lead hides a scenario with `delegate hidden add <scenario folder>`, as
   `/evaluate` does.
2. On machine A, the user edits another hidden scenario by hand in an editor, runs
   `delegate list`, then `evals/run fast` to try the edited check.
3. On machine B, `evals/run fast`.

## Passes when
- After step 1 the scenario is out of the repo, committed in the vault and on its remote.
- After step 2, `delegate list` names the hand edit as not on the remote yet, without
  contacting it; after `evals/run` the edit is committed and pushed, with no command typed for
  it (decision 011).
- On machine B, `evals/run` pulls first and runs the new and the edited hidden scenario.

## If this fails
Hidden scenarios drift apart between machines, and one machine's reviews miss checks the user
wrote on the other.

## Failure paths
- The remote can't be reached on machine A: the change is committed locally, the output says it
  isn't pushed and why, `delegate list` keeps saying so, and the next `evals/run` or
  `hidden add` that can reach the remote pushes it.
- Machine B can't reach the remote: `evals/run` runs its local copy and says it couldn't check
  that it's the latest.
- Both machines changed the same hidden file: the second push says there's a conflict and how to
  resolve it in the vault; neither change is lost.
