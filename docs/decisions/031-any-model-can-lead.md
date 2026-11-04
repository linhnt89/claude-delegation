# 031: Any model can lead; the lead names itself only for plan review

- Date: 2026-11-02
- Status: accepted; partly superseded by 038 (lead origin is recorded for observation only)

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
When the Claude quota runs out, the user wants another harness and model to lead (e.g. Pi
with GPT-6.1 Sol). The kit assumes a Claude lead in its texts: the lead's role is in
`CLAUDE.md`, the skills say "Claude plans", and they live under `.claude/skills/`. In the
code, the lead's identity is used in one place only: `plan-review` skips
`planner_family` from `workers.toml`. Review skips the implementer's family, which the task
already records. With a Sol lead and `planner_family = "claude"`, plan review skips Claude
and may pick Sol to review Sol's own plan.

## Decision
- **The kit is model-neutral.** The lead's role moves from `CLAUDE.md` to `AGENTS.md`, behind
  the same worker check (`.task/SPEC.md` exists → worker). The skills say "the lead", not
  "Claude". `CLAUDE.md` keeps `@AGENTS.md` and only what is Claude-specific.
- **delegate does not track who leads.** No session state, env var or harness detection.
- **`delegate plan-review --planner <family|model>`.** The lead passes its own model; a model
  is mapped to its family the same way worker families are. Without the flag,
  `planner_family` from `workers.toml` applies, as today. The delegate skill tells the lead
  to pass it.
- A non-Claude lead loads the skills from `.claude/skills/` by its harness's own means
  (Pi: `--skill <dir>`); the kit doesn't copy them elsewhere.

## Rejected options
- A per-session `DELEGATE_LEAD` env var — state to keep right for one reviewer choice.
- Detecting the lead from the harness's environment — fragile and implicit.
- A `delegate lead <worker>` launcher — changes no behaviour; a shell alias does the same.
- Copies of the skills under `.pi/`, `.agents/` etc. — two copies drift.
- Porting the Claude Code pane to other harnesses — `delegate watch` and `status` cover it.
- Steering workers away from the lead's account — worker order in `workers.toml` covers it.

## Consequences
Any harness that reads `AGENTS.md` can lead. Plan review stays independent only if the lead
passes `--planner`; forgetting it falls back to the machine's `planner_family`. A lead and
its workers may share one account's quota; the user orders candidates to avoid it.
