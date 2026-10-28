# Task <id>: <name>

<!-- Optional: task ids that wrote these checks; implementers skip their model families. -->
checks_by:
<!-- Optional: small | medium | long (default medium); a long task needs more quota runway. -->
size: medium

## Goal
<1–2 lines, including why: the why lets you make good judgment calls.>

## Context (read these, nothing more)
- `AGENTS.md`
- <docs/decisions/NNN-...md>
- <src/path/to/relevant/files>

## Scope
- In: `<path>`, `<path/**>`
- Out: <files/modules you must not touch>

## Contract
<Signatures, types, behavior the result must match. Paste only what isn't in the code yet.>

## Acceptance
<!-- Lines with a command in backticks are run by the runner; all must pass to merge.
     Put the user scenarios this task must satisfy here too: `evals/run S004 P002`. -->
- [ ] `<command>`
- [ ] `evals/run <scenario ids>`
- [ ] <observable behavior, checked in review>

## Constraints
- <No new dependencies / follow the pattern in X / no public API changes>

## If unsure
STOP. Don't guess. Write the question under "Open questions" in `.task/REPORT.md` and finish.

## When done
- Do not commit; the runner commits for you.
- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max.
