# S045 Copy a project folder: the copy starts clean and the original keeps its tasks
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) on a machine set up with `delegate setup --vault <url>` (a private remote), working on a project set up by the kit.

## Starting state
A project with one open task and hidden scenarios in the vault. The user copies the folder
(`cp -a shop shop-experiment`) to try something.

## Steps
1. `delegate list` in the copy, then in the original.
2. `evals/run fast` in the copy.
3. Delete the copy (`rm -rf shop-experiment`), then run any `delegate` command in the original.

## Passes when
- The copy shows no tasks; the original still shows its task.
- The copy's `evals/run` runs the same hidden scenarios (same project).
- Starting a task in the copy never touches the original's task state, and the other way round.
- After the copy is deleted, its task-state folder is cleaned up; the original's is untouched.

## If this fails
A copy would take over the original's tasks, so running or merging in one breaks the other.
