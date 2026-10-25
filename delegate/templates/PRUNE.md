# Task <id>: Prune stale docs

## Goal
Keep the persistent docs true and small so they don't mislead agents.

## Context (read these, nothing more)
- `AGENTS.md`, `CLAUDE.md`, `PLAN.md`, every `README.md` under `src/`
- `docs/decisions/` (headers and Decision sections only)

## Scope
- In: the files above
- Out: all code, tests and config

## Acceptance
- [ ] Every command in `AGENTS.md` was run and works, or was fixed or removed
- [ ] Every path in `AGENTS.md` and module READMEs exists
- [ ] Statements that contradict the code were fixed or deleted
- [ ] `PLAN.md` has no tasks marked done
- [ ] Decisions that a later decision contradicts are listed in REPORT.md
      (don't edit decision records; Claude decides on superseding)

## Constraints
- Prefer deleting a stale sentence to rewriting it longer.
- Don't add new documentation.

## If unsure
STOP. Don't guess. Write the question under "Open questions" in `.task/REPORT.md` and finish.

## When done
- Do not commit; the runner commits for you.
- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max.
