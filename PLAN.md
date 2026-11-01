# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Done
- Landed on main (2026-11-01, user present): t102, t086, t090, t104, t092 from autopilot run
  4f62f7e2. Then t105 (by Luna): the `.delegate.toml` template shows `[env] allow`; `AGENTS.md`
  lists `environment.rs`.
- t104, t092 (2026-11-01, user present): t104 fixed S108's check (with no gaps, its `printf`
  loop read an empty key under `set -u`), by Claude Haiku, S108's writers' family (`--worker`:
  the installed `delegate` predates t086). t092, the kit text for decision 025, by Sol, was landed
  by hand with the user's approval (its branch predates t104, and only t091 brings base fixes
  into a task); S108, S040 and `scripts/check` pass on `autopilot`. Its `fix-check` text is true
  once t091 merges.
- t090 (2026-11-01, user present): one allowlisted child environment for gates, acceptance and
  checks (`environment.rs`, `.delegate.toml` `[env] allow`, unknown keys refused); unit tests
  ignore ambient `DELEGATE_*`. By Sol, one round. The user dropped S085 from its regression line
  (S085 needs `autopilot away`; t095 runs it). S106, S084, S086, S087, S089, hidden P026 and both
  `scripts/check` lines pass; review 0 high, its lows are under Known, not fixed.
- t102, t086 (2026-11-01, user present): t102 fixed S098's check (its `evals/lib/helper.sh`
  fixture was outside the test project), by Copilot Sonnet: the user agreed Claude's family fixes
  it while Hyper has no login (exception to decision 024, like t099). t086, by Sol in four rounds
  (deep from round 2): check writers' families from history, hidden checks included (vault,
  `_evals/hidden`, `EVALS_HIDDEN`), across folder renames; `merge` refuses an implementer's
  changes to its Acceptance checks unless `fixes_checks:` names them and warns on `evals/lib/`;
  a check fix picks only from the named checks' writers. Round 3 fixed a bug hidden P025 caught
  (a fix task also excluded same-job hidden-check writers, leaving no candidate). S096–S099, P025
  pass; final review 0 high, its two mediums are t103.
- t087, t099, t100, t101 (2026-11-01, user present): t099 fixed S097's and S100's check bugs
  and moved S083, S085 (`away` after `start`), S086 (quota floor only) to decision 026; t100
  made S115 require a reverted task to stop counting as merged; t101 made the quota unit tests
  stop depending on today's date (two had failed since 2026-11-01, blocking every merge); t087's
  kit text then passed S100 unchanged. t099, t100 by Copilot Sonnet, t101 by Luna, t087 by Sol.
- t089, t094, t097, t098 (2026-11-01, wave 1): checks for S101–S105 (t089), S109–S113 (t094),
  S106, P026, S107, S108 (t097), S114–S118 (t098). Each fails on main for its stated reason; the
  lead read the failures in each REPORT and in `evals/run`. Merged as squash commits on
  `autopilot`. P026 and S116 moved to the hidden set (J3, J10), per the mandate.
- t093 (2026-11-01): scenarios S109–S118 agreed for decision 026, each with a pinned contract:
  `autopilot away`/`here`, present right after `start` (S109); `wait` returns on parked items and
  wave ends (S110); green waves land only while present (S111); a landing conflict lands nothing
  and the run goes on (S112); `autopilot answer` queues in the home, the next wave lead's prompt
  carries it (S113); red gate: one rerun (S114), revert and fix task (S115), still red stops
  (S116); the complete report (S117, the cut-off bug); kit text (S118). No checks yet.
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

## Handoff (2026-11-01, after the t103 discussion)
- Landed on main: t102, t086, t090, t104, t092, t105. `~/.local/bin/delegate` is still the build
  from before t086: reinstall it (README) before the next run, or `fixes_checks:` isn't applied.
- `delegate list`: no open tasks. Autopilot run 4f62f7e2 is stopped and landed.
- Decisions 028 (what a check is) and 029 (model fallback when families run out) accepted;
  scenarios S119–S129, P027 agreed with the user; t108, t107, t103 are in the mandate.
- Plan review (gpt-6.1-sol, wave 1) changes applied: t090's spec keeps the autopilot control
  variables for the lead and detached runs and adds the S084–S089 regression line. The t091/t095
  split and the Goal ids are still open (review finding 4): finish the split and list every piece
  id in the Goal before wave 2 and wave 3 start.
- Next: wave 2 (t091's pieces, t096, t108).

## Waiting for you
- Reinstall `~/.local/bin/delegate` from main (README), then `delegate autopilot start`.

## Mandate
<!-- Agreed with the user on 2026-11-01; renewed after wave 1 to add decisions 028, 029. -->
- Goal: t091, t095, t096, t108, t107, t103
- The goal includes the pieces t091 and t095 are split into.
- Gate: `env -u DELEGATE_AUTOPILOT -u DELEGATE_AUTOPILOT_WAVE scripts/check`
- Merge without asking when: checks, the task's and hidden scenarios, and required reviews pass.
- Needs the user: new decisions, scope changes, accepting findings, discarding a feature.
- Not in this run: t025 (needs a design with the user).
- Waves: 2 = t091's pieces, t096, t108; 3 = t095's pieces, t107; 4 = t103. t107 and t103
  both change candidate selection and `commands.rs`: t103 starts only after t107 merges, and
  t107 only after t091's pieces merge.
- Split t091 and t095 as the plan review below says (new ids from t109), before their wave;
  splitting them isn't a scope change.
  Start a task early only when its dependencies are merged and it shares no files with a
  running task.
- Check tasks (t108): before merging, confirm each check fails on main for the reason its
  `scenario.md` states (no setup error, no exit 3); a wrong reason is another round. Its spec
  carries decision 025's checklist: set up your own data, no assumptions about repo history,
  deterministic order, nothing taken from the ambient environment. t108 runs on a Claude-family
  worker, so t107's and t103's implementers skip Claude.
- Hidden: after t108's merge, `delegate hidden add` P027 and S127 (J3). Note them in
  `evals/jobs.md` under "Hidden"; t107 then runs them at review, not in Acceptance.
- `checks_by:` lines: t091 t089, t097; t095 t094, t098; t096 t098; t107 t108; t103 t108.
- Reviews: t091, t095, t107 and t103 merge only after a review on the final commit with no
  high-severity findings.
- Contracts for decision 029 (pinned with the user): model key = last segment of the model
  path, lowercased, effort and version dropped, Claude aliases mapped (`opus` =
  `claude-opus-5.5`); top-level `planner_model` beside `planner_family`; `[defaults]
  independence = "family" | "model"`, missing means `"family"`; trailer `Independence: model`
  after `Worker:`; `delegate list` shows `same-family fallback`. t107 also updates
  `delegate/templates/workers.example.toml` (both keys, and the `[review]`/`[plan_review]`
  comments).
- Answers for likely worker questions: a task's Acceptance scenarios are the ids on its
  `evals/run` lines; their checks are `evals/scenarios/<id>-*/check` (from t103 on, every file
  in the folder); hidden scenarios count when their `Job:` matches one of those scenarios'
  jobs; `fixes_checks:` parses like `checks_by:`; a fix-check worker that finds the check right
  writes `Status: check is right` in REPORT.md and changes nothing; autopilot's presence right
  after `start` is `present`.
- A check bug found during this run: park it with the failing output and the implementer's
  report until t091 merges, then use `delegate fix-check`.

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t091 | `delegate fix-check`; a task's next round includes check fixes merged into its base (decision 025) | 2 | todo | t086, t089, t097 | standard | `evals/run S101 S102 S103 S104 S105 S107`, `scripts/check` | touches `commands.rs` after t086 |
| t095 | Driver and commands for decision 026: `autopilot away`/`here`/`answer`, `wait` on wave ends and parked items, landing green waves while present, gate rerun and revert with a fix task, complete report | 3 | todo | t090, t091, t094, t098 | deep | `evals/run S109 S110 S111 S112 S113 S114 S115 S116 S117`, `scripts/check` | `autopilot.rs`, `wait.rs`; `checks_by: t094, t098` |
| t096 | Kit text for decision 026: start autopilot at "go", "I'm off"/"back", asking parked questions live, landing summaries | 2 | todo | t092, t098 | fast | `evals/run S118`, `scripts/check` | root and `templates/project/` together, then `scripts/kit-history` |
| t103 | Decision 028: every file in a scenario folder is its check; a check with no recorded writer gets any family's fix, with a warning; checks new in the task aren't its own | 4 | todo | t107 | standard | `evals/run S119 S120 S121 S122`, `scripts/check` | `check_writers.rs`, `commands.rs`; `checks_by: t108` |
| t108 | Checks for S119–S129, P027 (decisions 028, 029), on a Claude-family worker | 2 | todo | – | standard | each check fails on main for its `scenario.md` reason | `evals/scenarios/` only; decision 025's checklist |
| t107 | Decision 029: `independence` setting, model keys, same-family fallback for implement, review, plan review and check fixes, `planner_model`, `Independence: model` trailer, `list` shows it, hidden scenarios required at merge, `setup` hint, workers.example.toml | 3 | todo | t091, t108 | deep | `evals/run S123 S124 S125 S126 S128 S129`, `scripts/check`; hidden P027, S127 at review | `config.rs`, `check_writers.rs`, `commands.rs`; `checks_by: t108` |
| t025 | Codex workers bypass bwrap (`harness.rs:78`), so they can read the hidden set and the data folder today | – | todo | – | deep | a Codex variant of S048 (to write with the user) | needs a design: bwrap around Codex, or Codex's own read limits |

Plan review (gpt-6.1-sol, 2026-11-01), for the wave leads who write these specs:
- t090: its Acceptance `evals/run S106 P026` can't run P026 in the sandbox (hidden). Run P026 at
  review instead, and keep `evals/run S106` in Acceptance. Settle the stray-`DELEGATE_AUTOPILOT`
  question (Waiting for you) first.
- t091: too big for one diff (review 6): split question-phase handling, base update with
  conflict rollback, and `fix-check` orchestration. Also keep failure evidence per scenario, not
  the last 15 lines of each command (review 8), or S102's listing can lose early failures.
- t095: too big (review 5): split presence and answers, `wait`, live landing, and gate
  recovery. Its Acceptance lists `S116`, which is hidden now; run it at review instead. The
  S083/S085 migration in Waiting for you comes first.
- t092, t096: kit text.

## Known, not fixed
- Accepted at t090's merge (review, low): `ProjectRules::load` runs `[warm]` validation (git
  calls) on every check and gate, so a stale `[warm]` path fails `delegate check`; acceptance and
  check processes lose `_DELEGATE_AUTOPILOT_GROUP`, so `kill_leftover` can miss them; unit tests
  cover the env helper, not the call sites that pass `[env] allow`.
- Accepted at t086's merge (review, low): hidden checks are read from the first vault project
  folder only, without a pull (`evals/run` pulls first), so a check only in a duplicate folder or
  on the remote isn't counted; a hidden check outside git, or renamed and rewritten by more than
  half in one commit, loses its earlier writers.
- A round that only reruns checks after a check fix still steps the tier up (t087 went to
  `deep` on 2026-11-01): the failed round's checks counted against the product. `fix-check`
  (t091) should keep the tier when the failure was a check bug.
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
