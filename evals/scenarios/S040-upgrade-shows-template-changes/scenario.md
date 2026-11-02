# S040 Learn what changed in the templates of files the project owns
- Job: J1
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A developer (or Claude as lead) in a project set up with an older kit, who has just rebuilt `delegate` from a newer kit.

## Starting state
As S038. The new kit changed project-owned templates (`PLAN.md`, `AGENTS.md`, `CLAUDE.md`...)
outside their managed blocks; the project's own copies hold its content and were never meant
to be replaced.

## Steps
1. `delegate upgrade`.

## Passes when
- `PLAN.md`, `evals/jobs.md` and `.delegate.toml` are unchanged, and so is every byte of
  `AGENTS.md` and `CLAUDE.md` outside managed blocks (blocks follow S131–S133).
- The output names each project-owned file whose template changed outside managed blocks
  since the project's kit version and shows what changed, so the lead or the user can carry
  it over by hand.
- Templates that did not change are not mentioned.

## If this fails
Projects miss template changes that matter, such as the line that keeps workers in their
role, because upgrading only ever touches the kit's own files.

## Failure paths
- A project with no record of its kit version: the output shows the current template's
  difference from the project's file instead, and says it couldn't tell what's new.
