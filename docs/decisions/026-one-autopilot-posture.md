# 026: Autopilot is how the lead works on a mandate, present or away

- Date: 2026-10-31
- Status: accepted

## Context
Decision 019 made autonomy a separate mode. The user asks for a prep pass, commits a mandate,
runs `delegate autopilot start`, and later reads a report and runs `land`. Outside it the
lead stops at every merge and wave end (016). The first real run (2026-10-31) showed the
rest of the cost:
- the headless lead could only park what it didn't cause;
- any gate failure ended the run, even one from the environment;
- the report cut off what was waiting;
- nothing reached the user until the run stopped.

## Decision
- **One posture.** Once `PLAN.md` has a mandate and the user says go, the lead starts
  autopilot itself. The waves always run in the driver with a fresh headless lead per wave
  (019), present or away, so the user's session never grows with them.
- **Presence is what the user says.** The user tells the lead "I'm off" and "back", and the
  lead records it with `delegate autopilot away` / `here`. Nothing is inferred from idle time.
- **Present:**
  - A parked question reaches the user at once: the lead session waits on the run
    (`delegate wait` covers wave ends and parked items) and asks.
  - After each wave whose gate passed, the driver lands it on the base branch without asking
    (as `land`: fast-forward or clean merge, else nothing changes and the conflict is
    reported), and the lead session posts a short summary.
- **Away:** questions queue; one notification and a complete report when the run stops; the
  user lands once on return (019).
- **Answers** go through `delegate autopilot answer <task or item> "…"`. It writes to a queue
  in the delegate home, never to the autopilot branch. The next wave's lead records each
  answer where it belongs (the spec's `## Answers` or `PLAN.md`) and unparks the task.
  Answers are logged and listed in the report.
- **A red gate is triaged, not a stop.** The driver reruns the gate once. If it's still red,
  it reverts the last merge on `autopilot` with a revert commit, and the next wave gets a fix
  task for it, from the reverted commit and the gate output; dependents wait, other work goes
  on. If the gate is still red after the revert, the run stops: the cause isn't the merge.
- **Check bugs** go through `delegate fix-check` (025).
- **The report is complete:** each "Waiting for you" item in full, decisions to confirm,
  answers received, reverts and why.
- Without a mandate, the lead works as 016 says.

## Rejected options
- Inferring presence from session activity or idle time — wrong guesses either land work under
  a user who left or hold questions from one who is there.
- Asking before landing each green wave while present — the gate and the mandate's merge rule
  already passed; asking again is the stop the user wanted gone.
- On a red gate, only opening a fix task and parking — the gate stays red, every later merge
  fails it, and the run is stuck as on 2026-10-31.
- The user's session editing the autopilot branch to pass answers — it races the wave lead's
  own `PLAN.md` edits, which the lead lock exists to prevent.
- The user's session running the waves itself — its context grows (018, 019).

## Consequences
Landing changes the base branch under the user's editor while they are present; a conflict
lands nothing and is reported. A bad merge costs one revert and one fix task instead of the
run. The user answers parked questions as they come, so parked tasks wait about one wave. The
delegate skill teaches starting autopilot at "go" and the present/away words.
