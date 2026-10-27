# S062 A slow delegate never piles up reads or blanks the row
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer with several Claude Code sessions open, on a machine where `delegate status` is
slow (a slow vault remote, a busy disk).

## Starting state
A project with two running tasks. `delegate status --json` takes 10 seconds to answer.

## Steps
1. Keep a session open for a minute with the pane open, then with it closed.

## Passes when
- The session never runs more than one `delegate status` at a time.
- While a read is still running, the pane and the row keep showing the last result; the row
  never says `delegate: status unavailable` because a read is slow.
- Once a read answers, the pane and the row show it within the next refresh.

## If this fails
With `delegate` slow, every open session stacks up reads every few seconds, everything
gets slower, and the row flips to "status unavailable" with an error in the pane.

## Failure paths
- A read fails or gives up after its time limit: the row says `delegate: status unavailable`
  (S030), and the next good read replaces it.

## Properties
- None.
