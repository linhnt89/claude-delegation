# P010 Any worker output gives a working view
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Property
For any worker output (real recorded streams from each harness, cut at a random point, with lines garbled, unknown event types, very long lines or non-UTF-8 bytes mixed in), `status` still works: every well-formed step is shown, in order, nothing is made up from broken lines, and the run itself is unaffected.

## If this fails
One odd line from a worker breaks the view or, worse, the run.

## Properties
- This is the property.
