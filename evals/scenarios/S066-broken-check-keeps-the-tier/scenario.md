# S066 A broken check doesn't move the task to a stronger model
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead (or a developer) running a task whose acceptance includes a check that is itself
broken: for example a property generator that produces invalid input. Decision 013.

## Starting state
A task at standard. Its worker's work is fine, but one acceptance command exits 3: the check's
own way of saying "this is a bug in the check, not a verdict on the product".

## Steps
1. `delegate run t001`; the round finishes with that check error.
2. Look at the result (`delegate run`'s summary, `delegate status t001`, `delegate list`).
3. `delegate run t001` again, without `--tier`.

## Passes when
- The result calls it a check error, not a failure, and names the command that exited 3.
- The second round runs at standard again and says why: a check error, not a product failure.
- `delegate merge t001` is still refused while the check error stands: a broken check never
  counts as passing.

## If this fails
A bug in a check sends the task to the most expensive model, which can't fix it because the
product isn't what's wrong, and the user pays for rounds that change nothing.

## Failure paths
- A round with a check error and a real failure in another command: it counts as a product
  failure (it can step up).

## Properties
- See P019.
