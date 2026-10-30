# S092 With warm start on, the next task builds warm
- Job: J2
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user and the lead, running tasks one after another. Decision 020.

## Starting state
A project whose build writes a cache directory; warm start enabled in the project config; t001 merged after a build.

## Steps
1. `delegate run t002`, whose check looks for the cache t001's build left.

## Passes when
- t002's worktree sees the build cache from t001's build.
- With warm start off, t002 starts cold, exactly as today.

## If this fails
Every worker round rebuilds from scratch, costing minutes and quota each time.
