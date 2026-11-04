# Lead-owned S146 interface fixtures

Offline, redacted interface examples, NOT supplied-trace inputs for the live check
and NOT an S146 verdict. Workers may read these; do not alter the fixtures to make
an implementation pass. Unsupported or missing evidence remains a check error.

## Provenance

- `rollout-a.jsonl` / `rollout-b.jsonl`: projected from the completed credential-free
  real Codex 0.166.0 A/B rollouts recorded by the lead investigation documented in
  `docs/research/codex-credential-free-attribution.md`. Preserve original record
  ordering, timestamps, discriminants, prompt/reply and turn-link fields. Session,
  turn/message IDs and the run directory are replaced consistently. Unrelated
  records/fields (including context/instructions) are omitted. These are selected
  real-format records, not complete transcripts. Implementer tests must add preceding
  context-user messages, unrelated rows and truncated-tail cases separately.
- `processes.json`: real `daemon.pid`, `daemon-updater.pid` and simultaneous Linux
  `/proc` snapshots from the lead's fresh provisioning preflight on 2026-11-04.
  PID, process-start wall text, boot ID, start ticks, parent/group/terminal fields,
  address-valued stat fields, executableIdentity digest bytes and run prefix are
  redacted. All process-identity relationships are preserved. ExecutableIdentity
  bytes are placeholders, NOT expected binary SHA-256 values or proof of ownership.
  The preflight did not serve a model turn: these process snapshots alone do NOT
  prove rollout-fd ownership. There is intentionally no fabricated fd attestation.
- `/RUN` is an offline fixture prefix, never a permitted live root override.
- `proc.codex_home` is a projection of the matching NUL-separated environment entry,
  not a complete captured environment. Parser tests construct complete byte snapshots
  from this field. `cmdline` preserves NUL separators and actual argument layout.

## Additional lead captures

- `herdr-api.json`: real create/get/send envelopes and hook-review screen from a
  separate fresh credential-free capture, with run/binary paths and terminal ID
  redacted. The identity projection comes from the earlier real A sample, not a
  fabricated successful get response. Herdr closes ordinary RPC connections after
  ONE response: a persistent second request failed with BrokenPipeError. The driver
  must connect per request, within its fixed cycle budget.
- `local-responses.json`: actual POST path/body size and returned SSE bytes from
  that capture, plus the installed sole SessionStart declaration with path redacted.
  Request bodies are intentionally absent. Stub-generated response/message IDs
  are model objects, NOT thread IDs.
- `herdr-agent-state.sh`: genuine installed Herdr asset, byte-for-byte pinned hash;
  from herdrdev/herdr commit 2ecf19a92fc3a7fff75b6ce3655ef04f4625ec01,
  src/integration/assets/codex/herdr-agent-state.sh. Apache-2.0 license copied here.
  Test input only: do not execute it as a substitute live hook.
- `serving-fd.json`: separate completed real-turn capture with simultaneous actual
  daemon/PID/proc/fd/rollout inode+device relationship, projected and redacted.
  Run prefix, PID/boot/start/other stat numbers and inode/device replaced consistently;
  fd number and exact managed argv shape preserved. This is NOT an extra A/B run
  or proof of continuous serving-instance coverage. See live-interfaces research.

These fixtures pin interface shapes, not full live feature acceptance. Trusted
snapshot acquisition and continuity still need the reviewed real owner loop.

Expected rollout prompt: `Reply with LOCAL_STUB_OK. Do not use tools.`
Expected assistant reply: `LOCAL_STUB_OK`.
Both completed task records bind to their own started turn; the two session IDs differ.
