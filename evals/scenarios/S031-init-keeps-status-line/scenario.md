# S031 init leaves the status line alone
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
- Without an existing status line, `init` adds none either: tasks show in the row above the
  prompt (S030, S060), in the desktop app and in the terminal, so they aren't shown twice.
- `init` says, in one line, that `delegate statusline` can be added to a status line for
  those who want it there.

## If this fails
Setting up the kit silently breaks the user's own Claude Code setup.

## Failure paths
- A settings file that isn't valid JSON: left alone, with a message saying so.

## Properties
- Covered by P003 (init is idempotent) for repeated runs.
