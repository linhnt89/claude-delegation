# S011 A worker can't touch the main checkout, branches or hidden checks
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A repo with uncommitted work in the main checkout and hidden scenarios in the delegate home. A worker that tries to write to the main checkout, run `git branch -f main`, and read the hidden folder.

## Steps
1. `delegate run t001`.

## Passes when
- All three attempts fail inside the worker.
- The main checkout, including uncommitted files, is byte-for-byte unchanged, and every branch points where it did.

## If this fails
A worker could wreck the user's open work or history, or read the hidden checks and tailor its work to them.

## Failure paths
- None beyond the above.

## Properties
- A run never changes the main checkout or any branch other than its own task branch.
