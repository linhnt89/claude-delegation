# S118 The kit teaches starting autopilot at "go" and the present/away words
- Job: J9
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A lead in a project set up with `delegate init`. Decision 026.

## Starting state
A fresh `delegate init` project, and this repo's root kit files.

## Contract
In both the root files and `delegate/templates/project/`, the delegate skill says:
- once `PLAN.md` has a mandate and the user says go, the lead runs `delegate autopilot start` itself;
- "I'm off" → `delegate autopilot away`, "back" → `delegate autopilot here`;
- while present, wait with `delegate wait`, ask parked questions as they come, pass answers with
  `delegate autopilot answer`, and post a short summary after each landed wave.

## Passes when
- A lead following the kit never runs waves in the user's session.

## If this fails
Leads keep stopping at every wave, or run waves in the user's session until its context fills.
