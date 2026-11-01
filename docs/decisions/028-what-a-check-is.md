# 028: A check is its scenario folder; unattributed and new checks

- Date: 2026-11-01
- Status: accepted

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
t086 built decisions 023 and 024 on the file named `check`. Its final review found three gaps:
- S060–S065 keep their assertions in `mod.test.tsx` beside `check`, so an implementer can
  change them and merge, and their writers aren't counted.
- 69 of 117 checks have no `Worker:` trailer in their history (lead- or hand-written), so no
  family is their writer and no `fixes_checks:` task can ever get a worker, even with `--worker`.
- A check-writing task whose Acceptance names the scenario it creates is refused at merge: its
  new `check` counts as changing its own check.

## Decision
- **A check is every file in its scenario folder**, `scenario.md` included: for merge refusal
  (023), writer families (023, 024) and what `fixes_checks:` may change. Lead commits carry no
  `Worker:` trailer, so they add no writer family.
- **No recorded writer:** a `fixes_checks:` task for a check with no writer family may use any
  candidate in its tier, with a warning naming the check. Once the fix merges, its `Worker:`
  trailer makes that family the writer and 024 applies as usual.
- **New checks aren't the task's own:** only checks that exist in the task's base count when
  merge decides whether a task changed its own Acceptance checks.

## Rejected options
- Listing a scenario's extra files one by one — breaks the next time a scenario adds a fixture.
- Counting trailer-less commits as Claude's — it would suddenly bar Claude implementers from
  every task those checks judge, and some were written by hand.

## Consequences
Implementers can't change any file of their Acceptance scenarios, `scenario.md` included (the
AGENTS.md rule becomes enforced). Lead-written checks become fixable. Check tasks may list the
scenarios they create in Acceptance. Amends 024. Scenarios S119–S122; t108 writes their checks, t103 implements it.
