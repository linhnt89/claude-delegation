# S002 Run `init` again on a project that already has its own files
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A repo already set up, where the owner edited `AGENTS.md`, `PLAN.md` and `workers.toml`.

## Steps
1. Run `delegate init` again.

## Passes when
- Every existing file is byte-for-byte unchanged.
- The output says which files it skipped because they exist.

## If this fails
Running `init` again would wipe the user's own rules, plan or model choices.

## Failure paths
- A missing file among existing ones (e.g. `evals/run` deleted) is added back, the rest left alone.

## Properties
- `init` never changes a file that already exists.
