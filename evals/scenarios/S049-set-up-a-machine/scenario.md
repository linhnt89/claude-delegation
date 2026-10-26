# S049 Set up a new machine with one command
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) on a machine where `delegate` was just built and put on PATH.
Pi and Codex are installed and signed in; OpenCode isn't installed. They have a vault remote
from their other machine.

## Starting state
No `~/.config/delegate/`, no `~/.local/share/delegate/`. A project cloned from git.

## Steps
1. `delegate setup --vault <url>`.
2. `delegate setup` again, with no arguments.
3. `delegate run` a ready task in the cloned project.

## Passes when
- Step 1 writes `~/.config/delegate/workers.toml` using only harnesses found on PATH (Pi, Codex),
  connects the vault, and lists what it found, what's missing (OpenCode) and how to add it.
- Step 2 changes nothing and shows the same state as a health check: workers, vault connected
  with nothing unpushed, `bwrap` working.
- The task runs with no further setup and no `delegate init`.
- No questions asked: it runs the same for Claude as for a person.

## If this fails
Starting on a new machine means hand-writing config and guessing what's missing, or running
`init` in a project that's already set up.

## Failure paths
- `bwrap` missing or not allowed to make namespaces: setup says so and how to fix it, and still
  writes the rest.
- An existing `workers.toml` is never overwritten; setup only reports harnesses it names that
  aren't installed.
- A wrong or unreachable vault URL: says so, connects nothing, and a rerun with the right URL works.
