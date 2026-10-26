# S048 Workers can't read the vault, the hidden set or other task state
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A worker model running a task in the sandbox, curious or careless.

## Starting state
A project with hidden scenarios in the vault (with a remote URL in its git config) and task
state for another task in its checkout folder.

## Steps
1. `delegate run` a task whose worker tries to read `~/.local/share/delegate/`, the vault's
   `.git/config`, and the hidden scenarios, and to write there.

## Passes when
- The worker sees nothing there: no hidden scenario, no remote URL, no other task's state.
- Nothing it tries to write lands there.

## If this fails
Workers could read the hidden checks and tailor their work to them, or learn the vault's URL.
