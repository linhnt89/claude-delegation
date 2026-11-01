# S106 Gates, acceptance commands and checks don't inherit the caller's stray environment
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead (or autopilot's lead) running checks on a task. A bug: on 2026-10-31 autopilot's
gate failed because `DELEGATE_AUTOPILOT` leaked into `cargo test`. Decision 025.

## Starting state
A task whose gate, Acceptance command and a scenario check each write their environment to a
file. The caller's environment has `DELEGATE_AUTOPILOT`, `DELEGATE_AUTOPILOT_WAVE`,
`UNLISTED_SECRET`, and `EXTRA_ALLOWED` (named in `.delegate.toml`'s allowlist extras).

## Steps
1. `delegate check <task>`.

## Contract
- None of the three commands sees `DELEGATE_AUTOPILOT`, `DELEGATE_AUTOPILOT_WAVE` or
  `UNLISTED_SECRET`.
- Each sees `PATH`, `HOME`, `LANG`, the toolchain homes that were set (`CARGO_HOME`,
  `RUSTUP_HOME`, `MISE_*`), `DELEGATE_HOME` if set, and `EXTRA_ALLOWED`.
- The same holds when the commands run under `delegate merge` and `delegate run`. There the
  autopilot variables come from a real autopilot run; a caller holding a stray
  `DELEGATE_AUTOPILOT` for a run that doesn't exist is still refused by `run`, `merge` and
  `discard` (decision 019's lead lock), and `check` ignores it.

## Passes when
- A check gives the same verdict whoever calls it.

## If this fails
Checks pass or fail depending on who runs them, so a gate can stop autopilot for nothing or
pass broken work.

## Properties
- P026.
