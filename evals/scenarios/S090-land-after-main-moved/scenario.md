# S090 Landing when main moved meanwhile
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, back. Decision 019.

## Starting state
Autopilot stopped with merges on `autopilot`; meanwhile the user committed on the base branch: (a) other files, (b) a file autopilot also changed.

## Steps
1. `delegate autopilot land`.

## Passes when
- (a) The base branch gets autopilot's merges and keeps the user's commits.
- (b) `land` refuses, names the conflicting files, says what to do, and leaves the base branch and the working tree untouched.

## If this fails
Landing clobbers the user's own work, or leaves the repo half-merged.
