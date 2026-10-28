---
name: evaluate
description: Build and keep the project's evaluation set — runnable user-level scenarios and properties in evals/ that check the product the way its users use it. Use when the user says /evaluate, "define the evals", "what does done mean for users", when a new feature is being planned (its scenarios come first), when a bug is reported (it becomes a scenario), at review of delegated work (run the task's scenarios plus hidden ones), or before a release.
---

# Evaluate

The evaluation set says whether the product does its job **for its users**. Tests and lint
show the code behaves; they don't count here. It is defined before the code it checks, it is
runnable (`evals/run`), and it changes whenever the product does. Format: `evals/README.md`.

You draft; the user approves. Scenarios record **their** intent, so you are never the
authority on what they want. Code shows what the product does, not what it should do.

## Modes
| Say | Do |
|---|---|
| `/evaluate define` | Build the set from nothing (steps 1–6) |
| `/evaluate feature <name>` | Scenarios for a planned feature, before its specs (steps 2–5) |
| `/evaluate bug <description>` | Turn a reported bug into a failing scenario first (step 7) |
| `/evaluate review <task>` | Run the task's scenarios and hidden ones on its worktree (step 8) |
| `/evaluate run [tier]` | Run and report (step 6) |

## 1. Users and jobs (interview)
Ask, one topic at a time, and write down their words:
- Who uses this, in what situation?
- What are the 3–5 things they come to do?
- What does "it worked" look like to them?
- What would make them give up?
Write `evals/jobs.md`. Show it and get a yes before going on. If the user can't answer for
their users, record the guess as a guess and mark which jobs need checking with real users.

## 2. Scenarios
For each job, draft 2–4 scenarios from `evals/scenarios/_template/scenario.md`: the common
path, the costly mistake, the odd order or interruption. Each one needs a realistic starting
state, a `Passes when` that a user could observe, and an `If this fails` that says what a
failure would mean for the user in plain words. Number them `S001`... in a new folder
`evals/scenarios/S001-<slug>/`, `Status: draft`.

Show them as a short list (id, one line each) and let the user cut, add and reword. Approved
ones become `Status: agreed`. Don't write checks for unapproved scenarios.

## 3. Properties
From each agreed scenario, pull the rules that must hold for any input: invariants ("balances
sum to zero"), round trips (export then import gives the same data), idempotence (paying twice
charges once), metamorphic rules (a stricter filter never returns more), never-breaks (any
input gets a valid result or a clear message). Write each in plain language as a `P` scenario
with a `## Property` section, `Judged by: auto`. The user approves the sentences, not the code.
A property must be stronger than "returns something", and simpler than the code it checks.

## 4. Checks
Write `check` for each agreed scenario and make it executable:
- Drive the real interface: run the CLI, call the HTTP API, use a browser. Never import the
  product's internals; that tests the code, not the product.
- Build the starting state in `$EVALS_WORK` with realistic data; put shared setup in
  `evals/lib/`.
- Properties use a property-based testing library when the project has one (Hypothesis,
  fast-check, proptest...), otherwise a seeded random loop. Generate realistic inputs, not
  only random numbers. Print the seed and the smallest failing input.
- Workflows with state over time (carts, editors, sessions) get an action-sequence check:
  random sequences of user actions, compared after each step with a simple model of what
  should be true.
- Exit 3 for bugs in the check itself (bad setup, invalid generated input, flakiness), not for
  product failures. `evals/run` reports a `check error`, exits 3 when no product check failed,
  and exits 1 if anything failed. Check errors block merge but do not count toward a worker
  tier step-up; fix the check rather than sending the product to a stronger model.
- `rubric` checks ask a judge model with the criteria from `scenario.md`; `human` checks print
  what to look at and exit 2.
Set `Status: automated` once `evals/run <id>` gives a real verdict. Pick the tier by cost:
seconds to a few minutes is `fast`.

## 5. Hidden scenarios
Hide one or two automated scenarios per job, preferably properties and failure paths, with
`delegate hidden add evals/scenarios/<ID>-<slug>`. This moves the folder into the private
vault, commits it and pushes when connected; workers can't read it. Note the ids, not their
content, in `jobs.md` under "Hidden". They catch work tailored to the visible checks.

## 6. Run and baseline
`evals/run fast` (or `full`, `all`). On a new set, failures are expected: they are the work to
do, not broken checks. A check that fails for a reason unrelated to the product (setup,
flakiness) gets fixed now.

**Reporting to the user:** they shouldn't need to remember what an id means. Lead with what
each scenario is about and, for failures, what it means for their users; an id only goes in
brackets after that. Group by job, not by id. After building or changing the set, say how it
was built (whose answers, which scenarios they approved, what was inferred) so they can judge
how far to trust it.

## 7. Bug → scenario
Write the bug as a scenario (or property) that fails on the current product, confirm it fails
with `evals/run <id>`, get the user's yes, then fix or delegate the fix. It stays in the set.

## 8. Review of delegated work
For a task in review, in its worktree: `evals/run <ids from the spec>` and the hidden ones that
touch the same jobs (the runner finds them from the worktree). A failure blocks the merge:
write a next round that says what was run, what was expected, what happened. Never relax a
scenario to make work pass; changing one needs the user's yes.

## Keeping it current
- A feature is planned → `/evaluate feature` before any spec. Specs put scripted user checks in
  Acceptance (`` - [ ] `evals/run S004 P002` ``), so the runner gates merge on them.
- A feature is deliberately changed → update or retire its scenarios in the same change.
- A bug is reported → step 7.
- Before a release → `evals/run full`, plus a short hands-on pass by the user over the jobs.
