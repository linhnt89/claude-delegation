# S050 Claude as lead can't edit the product itself
- Job: J9
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude Code as lead in a project set up with `delegate init`, after the plan is agreed.

## Starting state
The shop project: `shop/pricing.py`, `tests/`, `PLAN.md`, `docs/decisions/`, `evals/` with a
scenario. `.claude/settings.json` holds the hooks `init` installed and nothing else changed.

## Steps
1. The lead tries Edit, Write and NotebookEdit on product files: `shop/pricing.py`, a new
   `tests/test_split.py`, a new folder `shop/api/handlers.py`, `evals/run`, a scenario's `check`,
   `.claude/settings.json`, `.delegate.toml`.
2. The lead edits its own files: `PLAN.md`, `docs/decisions/009-x.md`, `evals/jobs.md`, a
   scenario's `scenario.md`, `CLAUDE.md`, `AGENTS.md`.

Claude Code runs the project's hooks exactly as `.claude/settings.json` declares them.

## Passes when
- Every step-1 edit is refused and the file is unchanged.
- The refusal names the file and says what to do instead: write a spec and `/delegate` it.
- Every step-2 edit goes through.

## If this fails
Claude can quietly do the implementation itself, so the user's other models sit unused and the
work skips spec, checks and review.

## Failure paths
- A path outside the project (Claude's scratchpad, `/tmp`): allowed; the guard is about the repo.
- A path that leaves the lead's files by `..` or a symlink (`docs/../shop/pricing.py`,
  `docs/link` pointing at `shop/`): refused.

## Properties
- P017: the verdict follows the paths for any file and tool.
