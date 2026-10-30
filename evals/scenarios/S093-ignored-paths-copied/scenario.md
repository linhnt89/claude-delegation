# S093 Chosen ignored paths come with a new task worktree
- Job: J2
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user and the lead. Decision 020.

## Starting state
A main checkout with an ignored `.venv/` (and another ignored folder not named); warm start names `.venv/`.

## Steps
1. `delegate run t001`, whose worker changes a file in `.venv/`.

## Passes when
- The new worktree has `.venv/` with the main checkout's contents; the unnamed ignored folder isn't copied.
- The main checkout's `.venv/` is unchanged after the run.

## If this fails
Workers start without the environment the project needs, or a worker's change leaks into the user's checkout.
