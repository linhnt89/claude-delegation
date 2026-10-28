# S079 Starting a task whose files overlap a running task warns, and still runs
- Job: J5
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead starting a wave of tasks in the background. Decision 016.

## Starting state
t001 is running (stand-in worker kept busy) with `Scope In` listing `src/cart.rs` and
`src/price.rs`. t002's spec lists `src/price.rs` and `src/tax.rs`; t003's lists `docs/**` only.

## Steps
1. `delegate run t002 --background`.
2. `delegate run t003 --background`.

## Passes when
- Starting t002 prints a warning naming t001 and `src/price.rs`, and says how to avoid it
  (split the files apart, or run t002 after t001 merges).
- t002 starts anyway; the warning never stops a run.
- Starting t003 prints no overlap warning.

## If this fails
Two workers edit the same file at once and the clash shows up only at merge, as a conflict
round, or the lead is stopped from running tasks it judged safe.

## Failure paths
- t001 has finished (merged or discarded): no warning for t002.
- A glob in one spec (`src/**`) matching a path in the other counts as overlap.
- A spec with no `Scope In` paths: no warning, and no error.
