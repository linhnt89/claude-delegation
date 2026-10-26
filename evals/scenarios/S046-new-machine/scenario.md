# S046 Continue a project on another machine: the hidden set comes along
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
The same developer on a second machine: `delegate` built and on PATH, nothing else set up.

## Starting state
On the first machine the project has hidden scenarios in the vault, pushed to its private
remote. On the second machine, a fresh clone of the project.

## Steps
1. `git clone` the project; `delegate list` and `evals/run fast` in the clone.
2. `delegate setup --vault <url>` with the vault's remote.
3. `delegate list` and `evals/run fast` again.

## Passes when
- Before setup, both commands work and say, in one line each, that this machine isn't set up
  and this project's hidden scenarios aren't here, with the `delegate setup --vault <url>`
  command; neither creates a worker config or a vault.
- After setup, `evals/run` runs the same hidden scenarios as on the first machine.
- `delegate init` is never needed: the project's kit files came with the clone.
- No task state from the first machine appears on the second.

## If this fails
On a second machine, reviews would silently run without the hidden checks, so work tailored to
the visible ones gets merged.
