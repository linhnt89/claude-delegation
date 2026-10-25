# <Project name>

<!-- Read by every agent (Claude, Codex, Pi, OpenCode, Gemini) on every run.
     Keep it under ~150 lines. Only facts the code can't express, plus pointers.
     If a line here becomes false, fix or delete it in the same change. -->

## What this is
<One or two sentences: what the project does and for whom.>

## Source of truth
- **Code and tests are the truth.** If any doc conflicts with the code, follow the code
  and list the conflict under "Doc conflicts" in your REPORT.md.
- Why things are the way they are: `docs/decisions/` (append-only, newest wins).
- What we're working on now: `PLAN.md`.

## Commands
<!-- Exact, verified commands. Prefer one script per purpose. -->
- Setup:  `<e.g. make setup>`
- Test:   `<e.g. make test>`
- Lint:   `<e.g. make lint>`
- Check all before finishing: `<e.g. make check>`

## Layout
<!-- Only the top-level map. Module details belong in module READMEs/docstrings. -->
- `src/` — <...>
- `tests/` — <...>

## Conventions
<!-- Only what linters/formatters can't enforce. -->
- <e.g. errors are returned, never thrown, in the domain layer>

## Never
- Add dependencies without the spec saying so.
- Change public APIs, schemas or migrations outside the spec's scope.
- Commit secrets, push, deploy, or touch CI config unless the spec says so.
