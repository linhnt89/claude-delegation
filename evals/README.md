# Evaluation set

Whether the product does its job **for its users**, checked the way they use it: through the
real interface, with realistic data. Unit tests and lint don't count here. Built and kept
current with `/evaluate`; run with `evals/run`.

## Layout
```
evals/
  run                         runner: evals/run [fast|full|benchmark|all] [ID...]
  jobs.md                     who the users are and what they come to do
  lib/                        helpers shared by checks (setup, fixtures, judge prompts)
  scenarios/<ID>-<slug>/
    scenario.md               what is checked: the part people read and approve
    check                     executable: exit 0 pass, 1 fail, 2 needs a human, 77 skip
```
IDs: `S001`... for user scenarios, `P001`... for properties (rules that hold for any input).
Numbers are never reused; a dropped scenario stays with `Status: retired`.

## scenario.md
Header lines the runner reads:
```
# S003 Settle a group's debts
- Job: J2
- Tier: fast | full | benchmark
- Judged by: auto | rubric | human
- Status: draft | agreed | automated | retired
- Timeout: 300            (optional, seconds)
```
Then `## Who`, `## Starting state`, `## Steps`, `## Passes when`, `## If this fails`,
`## Failure paths`, and for properties `## Property`. `Passes when` is observable: what a user
would see. `If this fails` says in plain words what a failure means for the user; the runner
prints it under every failed result, so results read without looking anything up.

## check
Any language, executable, run in a fresh empty directory (`$EVALS_WORK`) with `$EVALS_ROOT`,
`$EVALS_LIB`, `$EVALS_SCENARIO` and `$EVALS_HIDDEN` set. It drives the product through its
real interface (CLI, HTTP, browser), never by importing internals. The last line it prints is
the reason shown in results.
- `auto`: decides by itself.
- `rubric`: asks a judge model to score against the criteria in `scenario.md`; prints the
  verdict and exits 0 or 1, or 2 if the judge is unsure.
- `human`: prints what to look at and exits 2.

## Tiers
| Tier | When |
|---|---|
| fast | Every change; minutes; can go in a spec's Acceptance as `` `evals/run fast` `` |
| full | Before a feature counts as done, and before a release |
| benchmark | Rarely: expensive comparisons |

## Hidden scenarios
A few scenarios live outside the repo in `<repo>.delegate/_evals/hidden/scenarios/`, same
format. Workers can't read that folder, so they can't tailor work to those checks. The runner
includes them when it can see them (the lead's review), and skips them inside the sandbox.

## Results
`evals/run` prints a table and writes it, with one log per check, to
`<repo>.delegate/_evals/results/`. Only the latest run per selection is kept; results are
review-time state and never committed.
