# S095 I can see what the autopilot lead is doing
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, checking on an autopilot run from a terminal or Claude Code. Decision 019.

## Starting state
Autopilot is running wave 2 of a plan. The stand-in lead has printed a stream-json with a few
steps: reading `PLAN.md`, running t003, reviewing it.

## Steps
1. `delegate watch`, `delegate list`, and the row above the prompt (`delegate statusline`
   output used by the mod); the pane's autopilot card.
2. The lead prints another step.
3. Autopilot stops with one task parked under `## Waiting for you`.

## Passes when
- Each view shows an "autopilot" line: the wave (2 of N) and the lead's latest step.
- After the lead's next event, the views show it within a few seconds.
- The pane's autopilot card shows the last few steps.
- After the stop, the line shows the stop reason and "waiting for you: 1".
- In a project that never used autopilot, nothing extra appears in any view.

## If this fails
The user can't tell at a glance whether the overnight run is working, stuck or done.
