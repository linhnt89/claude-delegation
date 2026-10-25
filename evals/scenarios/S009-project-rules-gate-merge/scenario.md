# S009 Project rules on changed paths gate the merge
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
`.delegate.toml` requiring `review = "always"` and `min_tier = "deep"` for `src/auth/**`; a spec whose Scope doesn't mention auth, but the worker changes `src/auth/`.

## Steps
1. `delegate run t001` at standard.
2. `delegate merge t001`.

## Passes when
- Merge is refused, naming the rule and the path that triggered it.
- After a deep round and a review on the final commit, merge is allowed.

## If this fails
Sensitive code lands with a cheap model and no review, because the spec didn't predict which files would change.

## Failure paths
- A high-severity review finding: merge refused unless `--accept-findings`.

## Properties
- None.
