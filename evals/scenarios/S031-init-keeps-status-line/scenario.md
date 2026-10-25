# S031 init keeps the user's own status line
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They already have a Claude Code status line of their own.

## Starting state
A project with `.claude/settings.json` holding a custom `statusLine` and other settings.

## Steps
1. `delegate init`.

## Passes when
- Their status line and every other setting are unchanged.
- `init` says it didn't install delegate's status line and how to add it (for example, the command to append to their own).
- Without an existing status line, `init` installs delegate's.

## If this fails
Setting up the kit silently breaks the user's own Claude Code setup.

## Failure paths
- A settings file that isn't valid JSON: left alone, with a message saying so.

## Properties
- Covered by P003 (init is idempotent) for repeated runs.
