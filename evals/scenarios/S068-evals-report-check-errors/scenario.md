# S068 `evals/run` tells a broken check apart from a failing product
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Anyone running the evaluation set, and the runner gating a task on `evals/run <ids>`.
Decision 013.

## Starting state
A project with the kit's `evals/` and three scenarios: one passing, one whose check fails, one
whose check exits 3 (a bug in the check).

## Steps
1. `evals/run <the passing one> <the check-error one>`.
2. `evals/run` on all three.

## Passes when
- The check error is listed as "check error", with the check's last line as the reason, and
  is never shown as pass, fail or skip.
- Step 1 exits 3; step 2 exits 1 (a real failure outweighs a check error).
- The summary line counts check errors on their own.
- `evals/README.md` and the `check` exit codes in `evals/run --help` list 3.

## If this fails
A bug in a check reads as a product failure, so people (and the runner) fix the wrong thing.

## Failure paths
- None beyond the above.
