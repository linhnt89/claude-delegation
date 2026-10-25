# Multi-model cooperation kit

## What this is
`delegate`, a Rust CLI that hands task specs to worker models (Pi, Codex, Claude Code,
OpenCode, Gemini) in sandboxed git worktrees, checks and merges their work; plus the project
templates and skills it installs. Users: developers who lead with Claude Code and want their
other models doing the implementation.

## Source of truth
- **Code and tests are the truth.** If any doc conflicts with the code, follow the code
  and list the conflict under "Doc conflicts" in your REPORT.md.
- Why things are the way they are: `docs/decisions/` (append-only, newest wins).
- What we're working on now: `PLAN.md`.
- What users need: `evals/jobs.md` and `evals/scenarios/`.

## Commands
- Check all before finishing: `scripts/check` (clippy, all + pedantic as errors; unit tests)
- User scenarios: `evals/run S004` (one), `evals/run fast` (all fast ones)
- Rust is pinned in `delegate/mise.toml`; run cargo through `mise exec --`.

## Layout
- `delegate/src/` — the CLI: `commands.rs` (subcommands), `config.rs`, `harness.rs` (worker
  commands, bwrap), `task.rs` (state), `status.rs` (live state, `status`), `watch.rs`,
  `statusline.rs`, `spec.rs`, `prompts.rs`, `init.rs`
- `delegate/templates/` — spec/report templates; `project/` is what `delegate init` installs
- `evals/` — user scenarios with executable checks; `evals/lib/` shared setup and stand-in workers
- `evals/lib/streams/` — recorded worker streams for checks; their `.steps` files are written by the lead, never by an adapter's author

## Conventions
- Errors go through `anyhow` with `bail!`/`context`; messages say what to do next.
- `unsafe` is forbidden; no `unwrap()` outside tests.
- `delegate/templates/project/` mirrors the kit files at the repo root (skills, `evals/run`,
  templates): change both together.

## Never
- Add dependencies without the spec saying so.
- Change public APIs, schemas or migrations outside the spec's scope.
- Change a scenario's `scenario.md` or `check` to make it pass.
- Commit secrets, push, deploy, or touch CI config unless the spec says so.
