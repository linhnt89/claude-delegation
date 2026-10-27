# S065 Each card in the pane shows who is doing the task, however long its title
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer leading from the Claude Code desktop app with the side pane docked narrow beside
the chat, running tasks on more than one harness.

## Starting state
Four tasks in this session, each with a spec title longer than the pane is wide and a
different worker: one running, one stalled, one ready to merge, one not started. The pane is
about 40 columns wide.

## Steps
1. Open the pane and read each card.

## Passes when
- Every card, in every state, has its worker (`harness:model@effort`) in full on a dim line
  of its own, right under the line with the id and title, before the Goal.
- The id and title line no longer carries the worker; the title is what gets cut at the edge.
- A task with no worker recorded has no empty line where the worker would be.

## If this fails
With a long title the user can't tell which harness and model is running a task without
leaving the pane, which is the one thing they look at to decide whom to trust with a merge.

## Failure paths
- None.

## Properties
- None.
