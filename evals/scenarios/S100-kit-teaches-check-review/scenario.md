# S100 The kit teaches the lead to check why new checks fail
- Job: J9
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead in a project set up by the kit. Decision 023.

## Starting state
A project after `delegate init`, and one after `delegate upgrade` from the previous kit.

## Steps
1. Read the installed delegate and evaluate skills and the REPORT template.

## Passes when
- The evaluate skill says a worker writes the checks, not the lead.
- The delegate skill says: a check task's REPORT lists each check with the failure it gives on
  the current product, and the lead confirms each fails for the reason its `scenario.md` states
  before dependent work starts; implementers report check bugs instead of changing checks; a
  check fix goes to its own task with `fixes_checks:`.
- The REPORT template has a place for that list.

## If this fails
Broken checks reach the implementer, cost rounds, and get fixed by the model they judge.
