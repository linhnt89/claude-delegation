# S001 Set up a new project with one `delegate init`
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A git repo with real code and a commit or two, no kit files yet; no `~/.config/delegate/workers.toml`.

## Steps
1. Run `delegate init` in the repo.

## Passes when
- `AGENTS.md`, `CLAUDE.md`, `PLAN.md`, `.delegate.toml`, `docs/decisions/`, `evals/` and the three skills exist.
- On a machine that isn't set up, no `~/.config/delegate/` file is written: the output ends by
  saying this machine isn't set up, with `delegate setup` (decision 008).
- The output lists each file it added.
- The repo's own files are unchanged.

## If this fails
A new user can't get started with one command, or `init` touches their code. They'd give up before the first task.

## Failure paths
- Run outside a git repo: a clear message that it needs one, nothing written.

## Properties
- Running `init` twice leaves everything as after the first run.
