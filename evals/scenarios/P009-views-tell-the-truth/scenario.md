# P009 Every view tells the same, true story
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Property
For any sequence of runs (foreground or background) whose workers step, finish, fail or get killed at random moments: at every moment `status`, `status --json`, `watch`, the status line and `list` agree on each task's phase, and that phase is true. A task shows as running only while its process is alive, as done only after it finished, and never as "not run" once it has started.

## If this fails
The views can't be trusted, so the user goes back to checking by hand.

## Properties
- This is the property.
