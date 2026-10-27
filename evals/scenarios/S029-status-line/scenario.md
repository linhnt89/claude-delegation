# S029 The Claude Code status line shows running tasks
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from Claude Code in the terminal and added `delegate statusline` to their status line themselves (`init` doesn't, S031).

## Starting state
A project set up with `delegate init`, its `.claude/settings.json` status line set to `delegate statusline`; two tasks running, one ready to merge.

## Steps
1. Run `delegate statusline` the way Claude Code runs it.

## Passes when
- It prints one short line: each of this session's tasks by id and phase (and stalled when it is), and how many of them are ready to merge; other sessions' tasks follow as dimmed counts (S055). With no session id on stdin, every task is this session's.
- With no tasks it prints nothing delegate-related.
- It answers fast enough not to slow Claude Code down (well under a second).

## If this fails
The user has to leave Claude Code to see that a task finished or got stuck.

## Failure paths
- Run outside a delegate project: prints nothing and exits 0.

## Properties
- None.
