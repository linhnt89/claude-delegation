# Task t133: Pure Codex rollout and process evidence readers

size: medium

## Goal
Provide small fixture-tested parsers for independent completed-thread and daemon
instance evidence. Keep process control, filesystem acquisition and the verdict
oracle outside this component.

## Context (read these, nothing more)
- `AGENTS.md`, `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-s146-provision-preflight.md`
- `evals/lib/fixtures/codex-identity/README.md` and its fixture files
- `evals/lib/codex_identity_oracle.py` (consumer boundary; do not modify)
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`

## Readiness
Run only after independent split-spec review; explicit `--tier standard`, offline
only. The lead has captured serving-fd evidence and pinned the t131 consumer below.
Projected fixtures are interface examples, not a live scenario verdict.

## Scope
- In: `evals/lib/codex_identity_evidence.py`,
  `evals/tests/test_codex_identity_evidence.py`
- Out: lead-owned fixtures, t130 oracle/tests, t132 preparation/stub, t131 owner loop,
  scenario.md/check wrappers, product/Rust/kit code, dependencies/worker configuration,
  authenticated data. No process, socket, network or filesystem access in this module.

## API

Standard library only. All arguments are in-memory snapshots supplied by t131.
`EvidenceError` has a stable redacted `code`; no input IDs, paths, prompt text or
raw parser exceptions in messages. Invalid input is never guessed into success.
Boolean values do not count as integers.

- `parse_rollout(data: bytes, *, prompt: str, reply: str) -> dict | None`: UTF-8 JSONL.
  Match exactly one user message containing the exact prompt in input_text and one
  assistant message containing the exact reply in output_text in the same turn.
  Context-user messages before the matching prompt are permitted; any additional
  user message after that prompt, repeated prompt, additional assistant message or
  second started turn is ambiguous. ONLY response_item/type=message role=user or
  assistant count for those message/ambiguity rules, NOT duplicate event_msg
  user_message/agent_message/item_completed records. Tolerate world_state,
  turn_context, token_usage_record, event_msg thread_settings_applied/item_completed/
  token_count/user_message/agent_message, response_item reasoning and developer
  messages. Full real record ordering is in rollout-a-full.jsonl; no records were
  omitted there, only content strings/identifiers redacted. A completed turn missing the expected messages
  is an error, not pending. Return None for pending
  incomplete evidence, including one incomplete final line while a writer is active.
  Complete malformed lines, conflicting metadata, invalid ordering or invalid field
  types raise EvidenceError. Result on completion is
  `{id, cwd, cli_version, model_provider, session_timestamp, turn_id, prompt, reply}`.
  Match one actual user prompt with task_started -> user input -> assistant reply ->
  task_complete, joined by matching task-start/completion turn_id/root_turn_id.
  Completion's last_agent_message and assistant output must agree. Session metadata
  `id` and `session_id` must agree when both are present. Context-user messages may
  precede the actual prompt; do not take the first role=user as the submitted turn.
  Extra unrelated record fields are tolerated, unsupported completion shapes aren't.
  Multi-turn/ambiguous evidence must raise, not select the latest turn. This helper
  never reads a Herdr identity or trusts a stub-supplied thread id.
- `bind_launch(rollouts: dict[str, bytes], before_paths: frozenset[str], *, cwd: str,
  launched_at: str, observed_at: str, prompt: str, reply: str) -> dict | None`:
  timestamps are timezone-qualified ISO8601 wall-clock snapshots; require launch <=
  observation. observed_at MUST be taken after inventory bytes have been read.
  A sole candidate outside [launched_at,observed_at] is check_error, not pending;
  clock reversal/drift must not create a binding. Inventory new paths only. Exactly one new candidate with supported
  metadata, matching cwd/version 0.166.0/provider local and metadata timestamp in
  [launched_at,observed_at] may bind. More than one new candidate (including auxiliary,
  incomplete or mismatched sessions) is ambiguous, not permission to select newest.
  Return None while the sole candidate has valid metadata but no complete turn;
  return the parse_rollout result only when exact expected prompt/reply are complete.
  No candidates is pending. A disappeared inventory entry is a check error. This
  function does not translate wall timestamps into oracle monotonic boundaries.
  It calls parse_rollout with supplied prompt/reply. Partial final lines are ignored
  only until newline; never ignore a malformed complete line. A complete turn
  followed by an incomplete tail can still return its completed evidence.
- `parse_pid_record(data: bytes, *, kind: str) -> dict`: kind is `daemon` or `updater`.
  Require JSON object, positive pid/startTicks, nonempty bootId, processStartTime
  string, actual observed processIdentity schema. Updater need not contain
  executableIdentity; digest arrays, if present, must be 32 bytes, not treated as
  SHA-256 executable proof. Return `{pid, boot_id, start_ticks}`. Reject legacy
  app-server.pid schema rather than guessing.
- `parse_proc_stat(data: str) -> dict`: parse Linux pid, parenthesized comm (which
  can contain spaces/parentheses), process state and field 22 start ticks; validate
  structure, integer bounds and reject zombie/dead state for liveness evidence.
  Return `{pid, start_ticks}`. No fixed whitespace offset over the entire line.
- `attest_process(record: dict, snapshot: dict, *, executable: str, codex_home: str,
  kind: str, rollout: str | None = None) -> tuple[str, int, int]`: snapshot keys
  `stat` (str), `boot_id` (str), `exe` (str), `cmdline` (bytes, NUL-separated),
  `environ` (bytes, NUL-separated), `fd_targets` (list[str]). Require PID/start/boot
  agreement; exact validated executable and private CODEX_HOME (not substring or
  duplicate env entry); cmdline[0] equals expected executable. Daemon args must be
  exactly `app-server --listen unix:// --analytics-default-enabled --managed-daemon`;
  updater args exactly `app-server daemon pid-update-loop`. Require canonical absolute expected paths and component
  containment beneath CODEX_HOME/packages. Reject `..`, deleted-exe/fd suffixes,
  mismatched argv and ambiguous env. If rollout is given, kind must be daemon,
  rollout must lie beneath CODEX_HOME/sessions, and its exact target must be in
  fd_targets. Return `(boot_id,pid,start_ticks)` for private driver tokenization.
  This is validation of snapshots, NOT a safe-to-kill authorization or race-free
  proof: t131 owns trusted collection, fd/inode binding, executable digest checks,
  stable identity rechecks and termination handles. A hash string in the PID JSON
  alone is never proof that a live executable is the pinned artifact.

## Tests

Fixture-driven success for the real-format rollout/PID/proc examples, plus synthetic
adversarial cases: preceding context users, unrelated messages, missing prompt,
wrong assistant/completion reply, wrong/mismatched turns, duplicate metadata/session,
partial final JSON vs complete invalid JSON, multiple new rollouts, reused old path,
wrong cwd/provider/version/time, bool/negative fields, legacy PID layout, changed
boot/start/PID, comm with parentheses, zombie, exe/env/cmdline mismatch, updater
mistaken for daemon, absent/wrong/deleted rollout fd, path containment lookalikes.
Assert no filesystem/process/network access and no mutation of caller snapshots.
Tests must distinguish snapshot validation from trustworthy acquisition; do not
claim fixture tests prove live serving-instance continuity.

## Acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_evidence.py' -v`
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `scripts/check`
- [ ] All listed adversarial classes have executable tests; independent review has
  no unresolved high findings.

These are offline infrastructure gates for agreed S146, NOT `evals/run S146` or a
user-level pass. The later t131 lead-only live baseline gate is still mandatory.

## Constraints / when done
No dependencies, filesystem/proc control, live input mode or product/schema change.
Stop on contract ambiguity; do not invent evidence. Do not commit; runner commits.
Report actual family, test counts, limits and doc conflicts in `.task/REPORT.md`.
