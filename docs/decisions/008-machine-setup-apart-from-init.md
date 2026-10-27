# 008: Machine setup is `delegate setup`, apart from `init`

- Date: 2026-10-26
- Status: accepted; "pushed by the next delegate command" superseded by 011

## Context
`delegate init` sets up a project but also writes the machine's `~/.config/delegate/workers.toml`
(S001), so it seems needed on every machine. Its project files are committed, so a clone never
needs `init`. Decision 007 adds a second machine-wide piece, the vault and its remote.

## Decision
- **Three scopes, one command each.**
  - Machine: `delegate setup [--vault <url>]`, once per machine and safe to rerun. Writes
    `workers.toml` from the harnesses found on PATH (never overwriting one), connects the vault
    to its remote (pushing when the remote is empty, merging when not), checks `bwrap` and
    each harness, and prints what's missing with how to fix it. With no arguments and nothing
    to do, it is a health check and changes nothing.
  - Project: `delegate init` (once per project, by whoever creates it) and `delegate upgrade`.
    No global changes.
  - Checkout: nothing to run; the checkout id and task-state folder appear on first use.
- **`delegate where`** prints this checkout's folders, one per line: `tasks <path>` and
  `hidden <path>` (or `hidden none: <why>`), so people and checks needn't know the layout.
- **No prompts.** Everything is flags, so Claude runs setup as well as a person.
- **The vault is created locally when first needed** (migration, the first hidden scenario);
  `delegate setup --vault <url>` connects it. `delegate hidden add <scenario folder>` moves a
  scenario into the vault, commits and pushes at once; hand edits in the vault are committed
  and pushed by the next `delegate` command. `evals/run` pulls before running hidden scenarios
  and runs the local copy, saying so, when the remote can't be reached.
- **On a machine that isn't set up**, commands keep working where they can and end with one
  line: `this machine isn't set up: run delegate setup`. They never create global files.

## Rejected options
- `init` keeps writing `workers.toml` — it makes `init` look per-machine, and a clone never
  needs it.
- Separate `vault init` / `vault clone` / `vault connect` — the user has to know which case
  they're in; one `setup --vault` covers all.
- A Claude Code hook syncing the vault after each edit — one commit per file edit, and it
  only sees Claude's edits.
- Interactive setup questions — Claude can't answer them.

## Consequences
S001 changes: `init` no longer creates `workers.toml` and instead points to `delegate setup`
(needs the user's yes). The kit's skills and README describe a new machine as: build, `delegate
setup --vault <url>`, clone.
