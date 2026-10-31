# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Done
- t088 (2026-10-31): scenarios S101–S108, P026 agreed for decision 025, each with a pinned
  contract: `fix-check` (S101, S102), "the check is right" closes the fix task at merge and sends
  the reason to the implementer (S103), fixes reach a running task (S104), question-phase check
  flags stop the run (S105), allowlisted child environment: basics, toolchain homes,
  `DELEGATE_HOME`, `.delegate.toml` extras (S106, P026), autopilot fixes checks (S107), kit text
  (S108). Unit tests ignoring ambient `DELEGATE_*` stays in t090's Acceptance. No checks yet.
- t085 (2026-10-31): checks for S096–S100 and P025 (decisions 023, 024), by DeepSeek. Each fails
  on the current product for the reason its `scenario.md` states (checked in the task worktree;
  the product matches main). P025 is hidden (J3). Merged as 386a041; the gate after it failed
  (see Waiting for you).
- t084 (2026-10-31): scenarios S096–S100, P025 agreed for decisions 023, 024 (check writers'
  families from history, merge refuses an implementer's check edits, check fixes by the writers'
  family with `fixes_checks:`, the kit's check review step). No checks yet.
- t001–t007 (2026-10-25): the 12 scenarios failing at baseline now pass; see git history.
  Every spec's Acceptance gets the project gate `scripts/check`; at review, `evals/run` adds
  the hidden scenarios for the same jobs.
- t008–t019 (2026-10-26): live view of running tasks (J8, decision 002): `status`, `watch`,
  `wait`, the status line, the side pane and the summary row above the prompt.
- t020 (2026-10-26): `delegate upgrade` and `.delegate-kit.toml` (J1, decisions 005, 006).
- t030–t032 (2026-10-27): each Claude Code session sees its own tasks, by title, in the status
  line and the pane, other sessions' apart (J8, decision 009).
- t021–t024 (2026-10-27): the delegate home survives moves and machines (J7, J1; decisions
  007, 008, 010): task state by checkout id, one vault repo for hidden sets, `delegate setup`,
  automatic migration of old homes. This repo migrated on 2026-10-27; its vault's remote is on GitHub.
- t026 (2026-10-27): checks for the lead guard (J9): S050–S054, P017 fail until the guard is
  built. A pilot of delegating eval writing; the `evaluate` role is not distilled yet.
- t040–t041 (2026-10-27): tasks show once, in the row above the prompt, with other sessions
  dimmed; the pane opens only on request and reads one status at a time; `init` adds no status
  line and `upgrade` removes the kit's (J8, J1, decision 012). t042: in the desktop app the row
  hands its whole text to the app to fit (S064). t043: each pane card shows its worker on its own dim line, so a long
  title can't push it off (S065). The user passed S030 and S057 in the
  desktop app on 2026-10-27.
- t033 (2026-10-27): S031's check compares the user's settings and hooks, not the file's bytes,
  so the guard can add its hooks (J1, J9). P017 is now hidden.
- t034–t038 (2026-10-27): the vault syncs only when the hidden set is changed or run (J8, J7,
  decision 011): `list`/`status`/status line went from 10–97 s to 0.03 s on a real machine;
  `evals/run` (via `where --pull`), `hidden add` and `setup --vault` sync, with hard network
  limits and one sync at a time. t035 (sync on every command, within a budget) was discarded.
  Checks by GPT, fix by Claude Sonnet, eight rounds and six cross-family reviews.
- t044–t048 (2026-10-28): decision 013. Checks that exit 3 are check errors and don't step a
  task up; `step_up_after` sets how many product failures come first; tasks never step down;
  implementers skip the family named by `checks_by:` (from `Worker:` merge trailers).
  Checks by DeepSeek (t047, t048), fixes by Sol; installed in `~/.local/bin` on 2026-10-28.
- t050–t055 (2026-10-28): decision 014. delegate reads Claude, ChatGPT (Pi/Codex), Copilot
  (Pi's login) and Hyper (Pi's API key) quota itself; `delegate quota`; runs skip accounts that
  are out or won't last the spec's `size:`; a worker out of quota mid-round hands over to the
  next candidate; all out → stop with resets. Checks by GPT (t050, t051, t054), fixes by
  DeepSeek, eleven cross-family reviews by Sol. Installed on 2026-10-28.
- t057, t058 (2026-10-28): bugs from a second machine. A Claude Enterprise seat (no plan
  windows) shows its monthly spend cap as a `monthly` window with no reset given: account ok,
  runway unknown (S077). Plan windows always win over extra usage. "Log in again" is shown
  only when the login failed. An HTTPS vault URL without saved credentials says to use SSH or
  save credentials (S078). Checks by Claude, fixes by DeepSeek.
- t056 (2026-10-28): the lead guard (J9, decision 015): `delegate guard` hooks refuse the
  lead's product edits and hold its turn after a shell edit; workers are told apart by the
  runner's `DELEGATE_WORKER`; `init`/`upgrade` install it, `DELEGATE_GUARD=off` turns it off.
  S050–S054, P017 pass. Checks from t026, fix by Claude Opus (Copilot) and Sol, review by DeepSeek.
- t064 (2026-10-30): checks for autopilot and warm worktrees (decisions 018–020, J10): S083–S094,
  P022–P024, all failing until built; a stand-in headless lead in `evals/lib/autopilot_lead.sh`.
  Checks by DeepSeek, three rounds (questions, then the plan-review additions).
- t082, t083 (2026-10-31): one read-only function decides whether autopilot is running
  (`autopilot::effective_status`), used by the views too. This repo copies `delegate/target`
  into each task as a block clone (decision 022, superseding 021's sccache, whose server could
  act outside the sandbox); it saves about 2 s of a 10 s build. By Luna.
- t080 (2026-10-31): S040's check works out which owned templates changed since the first kit
  (t071 changed PLAN.md and .delegate.toml). By Luna. `evals/run fast` on main: 102 pass, P018
  flaked once (passes alone).
- t075 (2026-10-31): `delegate autopilot land`: fast-forward or a clean merge into the base
  branch; on a conflict it changes nothing and names the files (S083, S090, hidden P022). By Sol.
- t072, t079 (2026-10-31): `list`, `watch`, `status --json`, the row above the prompt and the pane
  show an autopilot line: wave, the lead's latest step, a "quiet" hint, stop reason, waiting
  count (S095). By Claude Sonnet. t079 fixed S083's post-land check; S083 now allows parked
  tasks after land (agreed with the user).
- t070, t078 (2026-10-30): `delegate autopilot start/stop/status` (decision 019): a fresh headless
  Claude lead per wave in an `autopilot` worktree, a lead lock with per-run ids, mandate checks,
  gate after every merge, round limit, stop reasons, log, run report, one notify with timeout.
  S084–S089, P023, P024 pass. By Sol (five rounds: check bugs, then a Sonnet review). t078
  fixed P024's check and wrote S095's.
- t074, t077 (2026-10-30): warm task worktrees (decision 020): `.delegate.toml` `[warm]` `share`
  dirs live in the task home and are writable in the sandbox, `copy` paths are copied in; both
  must be ignored by git. t077 fixed two autopilot checks (sandboxed stand-in signalling, stale
  calls). By Sol and Luna. S092, S093 and hidden S094 pass.
- t071, t076 (2026-10-30): the kit teaches one session per phase with a `PLAN.md` handoff, and the
  autopilot prep pass, mandate, parking and lock (decisions 018, 019; S091). t076 fixed S091's
  check. By Sol.
- t060–t063 (2026-10-29): decisions 016, 017. The lead works in waves: the delegate skill and
  templates teach the batch loop (one id-less `wait`, work while workers run, one merge ask per
  batch); `run` warns when `Scope In` overlaps a running task; parallel tasks share an
  account's runway in tier order. Checks by DeepSeek (t060), fixes by Sol (t061, t063) and
  Luna (t062). S079–S082 pass, P021 hidden.

## Handoff (2026-10-31)
- Wave 1 ran t085 only: merged, P025 hidden, landed on main. The gate after it failed because
  the autopilot token leaks into `cargo test` (16 unit tests fail with "autopilot owns the lead
  lock"; with `DELEGATE_AUTOPILOT` and `DELEGATE_AUTOPILOT_WAVE` unset, 303 pass). Fix: t090.
- Decided since: decision 025 (pinned contracts, small check tasks, implementers review their
  checks first, `delegate fix-check`, fixes reach running tasks, clean child environment).
- Decided since: decision 026 (autopilot is how the lead works on a mandate, present or away:
  the lead starts it at "go", presence is what the user says, green waves land while present,
  answers through `delegate autopilot answer`, a red gate is rerun, then the merge reverted).
- t088 done (S101–S108, P026 agreed). Next: t093 with the user (scenarios for 026), then a
  new Mandate. t086 and t087 can run interactively meanwhile.
- `delegate list`: no open tasks.

## Mandate
<!-- On hold: rewrite after t088 and t093 (decisions 025, 026); t085 is done. -->
- Goal: t085, t086, t087
- Gate: `scripts/check`
- Merge without asking when: checks, the task's and hidden scenarios, and required reviews pass.
- Needs the user: new decisions, scope changes, accepting findings, discarding a feature.
- Not in this run: t025 (needs a design with the user).
- t085: before merging, confirm each check fails on main for the reason its `scenario.md`
  states (no setup error, no exit 3); a wrong reason is another round. After the merge, hide
  P025 with `delegate hidden add` and note it in `evals/jobs.md` under "Hidden".
- t086: merge only after a review on its final commit with no high-severity findings.
- Answers for likely worker questions: a task's Acceptance scenarios are the ids on its
  `evals/run` lines; their checks are `evals/scenarios/<id>-*/check`; hidden scenarios count
  when their `Job:` matches one of those scenarios' jobs; `fixes_checks:` parses like
  `checks_by:`.

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t086 | Runner: check writers' families from git history (hidden included) plus `checks_by:`; `merge` refuses a task that changes its own Acceptance checks unless `fixes_checks:` names them, warns on `evals/lib/`; a check fix runs only in its writers' families (decision 024) | 2 | todo | t085 | standard | `evals/run S096 S097 S098 S099`, hidden P025, `scripts/check` | `checks_by: t085` |
| t087 | Kit text for decisions 023, 024: `/evaluate` step 4 says a worker writes checks; delegate skill gets the check review step, "report check bugs, don't fix them" and `fixes_checks:`; SPEC template gets `fixes_checks:`; REPORT template lists a check task's failures | 2 | todo | t085 | fast | `evals/run S100`, `scripts/check` | root and `templates/project/` together, then `scripts/kit-history` |
| t089 | Checks for S101–S105 (decision 025: `fix-check`, check is right, fixes reach running tasks, question-phase flags) | 3 | todo | t088 | standard | each fails for the reason its `scenario.md` states | with the check-task checklist (decision 025) |
| t097 | Checks for S106, P026, S107, S108 (decision 025: clean child environment, autopilot fixes checks, kit text) | 3 | todo | t088 | standard | each fails for the reason its `scenario.md` states | with the check-task checklist (decision 025) |
| t090 | One allowlisted child environment for gates, acceptance and checks; unit tests ignore ambient `DELEGATE_*` (decision 025) | 4 | todo | t097 | standard | `evals/run S106 P026`, `scripts/check` with `DELEGATE_AUTOPILOT`, `DELEGATE_AUTOPILOT_WAVE` set | needed before the next autopilot run |
| t091 | `delegate fix-check`; a task's next round includes check fixes merged into its base (decision 025) | 4 | todo | t086, t089, t097 | standard | `evals/run S101 S102 S103 S104 S105 S107`, `scripts/check` | touches `commands.rs` after t086 |
| t092 | Kit text for decision 025: pinned contract in `/evaluate`, check-task limit and checklist, question-phase check review, "Check problems" REPORT section, `fix-check` (also for autopilot) | 4 | todo | t087, t097 | fast | `evals/run S108`, `scripts/check` | root and `templates/project/` together, then `scripts/kit-history` |
| t093 | Scenarios for decision 026, with the user: the lead starts autopilot at "go"; `autopilot away`/`here`; present: a parked question reaches the lead session through `delegate wait`, a green wave lands with a summary, a conflict lands nothing; `autopilot answer` unparks a task in the next wave; red gate: rerun, revert, fix task, stop if still red; complete report; kit text | – | todo | – | lead | `Status: agreed`, each with its pinned contract | `/evaluate feature`; the cut-off report via `/evaluate bug` |
| t094 | Checks for t093's scenarios | 3 | todo | t093 | standard | each fails for the reason its `scenario.md` states | at most 5 scenarios per check task: split if more |
| t095 | Driver and commands for decision 026: `autopilot away`/`here`/`answer`, `wait` on wave ends and parked items, landing green waves while present, gate rerun and revert with a fix task, complete report | 5 | todo | t090, t094 | deep | its t093 scenarios, `scripts/check` | `autopilot.rs`, `wait.rs`; after t090 |
| t096 | Kit text for decision 026: start autopilot at "go", "I'm off"/"back", asking parked questions live, landing summaries | 5 | todo | t092, t094 | fast | its t093 scenario, `scripts/check` | root and `templates/project/` together, then `scripts/kit-history` |
| t025 | Codex workers bypass bwrap (`harness.rs:78`), so they can read the hidden set and the data folder today | – | todo | – | deep | a Codex variant of S048 (to write with the user) | needs a design: bwrap around Codex, or Codex's own read limits |

## Known, not fixed
- P018 (hidden) failed again under a full parallel `evals/run fast` (2026-10-31: "t101 belongs to
  <session>, created by None"), passing on rerun: flakiness under load, not tied to a change.
- Accepted at t056's merge (review, medium): a shell edit by the lead from the main checkout
  into a task worktree isn't caught at `Stop` (it watches only the lead's own worktree, so a
  running worker's edits are never blamed). Edit/Write there are still refused.
- Accepted at t037's merge (review 6): a push address set by hand on the vault
  (`git remote set-url --push`) survives `setup --vault <new>`, so syncs keep pushing there
  and `list` reports nothing pending. delegate never sets one itself; fix: `setup --vault`
  clears it.
- Accepted at t037's merge (review 6, medium): during a push, git updates
  `refs/remotes/origin/*` before delegate retakes `vault.lock`, so it can land while another
  process (migration, plain `where` tidying) holds that lock. Syncs already take turns.
- Sessions are named by their checkout folder only: neither the CLI nor the mod reliably knows a
  session's title (the mod gets it at `SessionStart`; the app renames sessions later).
- Task ids are per repo, not per worktree: two worktree leads can take the same id, and
  `run --background` then reports "running" while the runner died on the existing `task/<id>`
  branch. t021 touches the home, so check whether it should claim ids there.
- An older `delegate` on PATH after a newer one migrated the home finds no hidden scenarios and
  only says "no hidden scenarios found".
- Accepted at t044's merge (review, low): a check-error round keeps the tier even if the
  product-failure count already reached `step_up_after` (only after `--tier` or lowering it);
  and `pick_tier` takes the highest tier of all rounds, research rounds included.
- A task that adds a crate fails `scripts/check` in the sandbox: `~/.cargo` is read-only there,
  so cargo can't download it (t052). Workaround: the lead runs `cargo fetch` in the task's
  worktree on the host, then `delegate check`. Fix idea: the runner fetches before checks.
- P018 (hidden) failed once in a full `evals/run fast` with "delegate new t100: No such file or
  directory" (seed 577651) and passes on replay with that seed: setup flakiness under load.
- `delegate/tests/quota_review_check.sh` and `quota_review_round3_check.sh` (review evidence from
  t053) aren't run by `scripts/check`; turn them into scenarios or Rust tests, or drop them.
- `delegate list` says "vault: N commit(s) not pushed" when the vault has no remote at all.
- A card beside the desktop app's Environment/Context cards isn't possible: the mod API
  (`claude-code.d.ts`, Claude Code 2.1.308) has no side-card component. Revisit when it does.
- Unverified: the guard lets through paths outside the repo, so the lead may be able to edit
  hidden checks in the vault.

## Open questions
- None.
