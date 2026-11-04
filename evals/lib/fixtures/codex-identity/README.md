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

## Coverage limits / outstanding captures

These fixtures establish rollout and PID/proc shapes only. A complete live-wave
fixture set still needs actual Herdr create/get/read responses over one persistent
socket, the hook-approval screen and raw SSE exchange, and a redacted snapshot
binding an open rollout fd to the serving process. Do not fill those gaps with
invented observations or call this directory full live-interface acceptance.

Expected rollout prompt: `Reply with LOCAL_STUB_OK. Do not use tools.`
Expected assistant reply: `LOCAL_STUB_OK`.
Both completed task records bind to their own started turn; the two session IDs differ.
