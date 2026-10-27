# P017 The guard's verdict depends only on who is editing and the path
- Job: J9
- Tier: fast
- Judged by: auto
- Status: automated

## Who
The guard, as Claude Code calls it.

## Starting state
The shop project with the guard installed, plus realistic extra files and folders: nested
source folders, dotfiles, names with spaces, symlinks.

## Property
For any file path and any of Edit, Write and NotebookEdit:
- from the lead, the edit is allowed exactly when the path, resolved (`..`, symlinks), is inside
  the lead's files (`PLAN.md`, `docs/**`, `evals/jobs.md`, `evals/scenarios/*/scenario.md`,
  `CLAUDE.md`, `AGENTS.md`) or outside the project;
- from a worker started by the runner, it is always allowed;
- every refusal names the path and says what to do instead.

## If this fails
Some product path slips past the guard, or some of the lead's own files are blocked, depending on
how the path happens to be written.

## Failure paths
- A malformed hook input: refused, with a message saying the guard couldn't read it.
