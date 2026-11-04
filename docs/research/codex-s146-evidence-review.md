FINDINGS: 2 high, 2 medium, 0 low

1. **High — `evals/lib/codex_identity_evidence.py:345`: Messages explicitly attributed to another turn are accepted.**
   Only task-start/completion IDs are compared. The real-format message attribution in `payload.internal_chat_message_metadata_passthrough.turn_id` and `metadata.retained_source.id.turn_id` is ignored; `turn_context` identities are also skipped at line 336. Thus record ordering alone can bind another turn's prompt/reply to the claimed completed turn, contrary to the same-turn and conflicting-metadata requirements.
   Reproduce: in `rollout-a-full.jsonl`, change both attribution locations on the actual user message (record 7) or assistant message (record 10) to `other-turn`, leaving start/completion unchanged. Parsing still returns completed `turn-A` evidence. Changing the turn_context turn/root IDs likewise succeeds. Added failing tests: `AdversarialReviewTests.test_review_messages_from_another_turn_are_rejected` and `test_review_conflicting_turn_context_is_rejected`.

2. **High — `evals/lib/codex_identity_evidence.py:367`: Assistant output need not agree with the completion.**
   The `any(...)` check accepts an assistant message whenever one output_text item equals the expected reply, regardless of additional contradictory output_text items. Completion is compared only against the supplied reply, not the actual complete assistant output. The result therefore reports the expected reply even when the observed assistant output differs from `last_agent_message`, violating the explicit agreement requirement.
   Reproduce: append `{"type":"output_text","text":"CONTRADICTORY_EXTRA_OUTPUT"}` to the valid assistant message while leaving completion.last_agent_message as `LOCAL_STUB_OK`. Parsing returns success. Added failing test: `AdversarialReviewTests.test_review_all_assistant_output_must_agree_with_completion`.

3. **Medium — `evals/lib/codex_identity_evidence.py:169` (also line 145): Non-JSON constants are accepted as valid JSON evidence.**
   Both json.loads calls retain Python's permissive handling of `NaN`, `Infinity` and `-Infinity`. These are invalid JSON tokens, but placing them in an otherwise unrelated field lets a complete malformed rollout line or PID object succeed. Tolerating unrelated fields does not satisfy the requirement to reject malformed complete JSON.
   Reproduce: insert `"extra":NaN` into a valid newline-terminated rollout record or daemon PID object; repeat with either infinity token. Both readers return success. Added failing test: `AdversarialReviewTests.test_review_non_json_constants_are_rejected` (six failing subcases).

4. **Medium — `evals/lib/codex_identity_evidence.py:122`: Redacted errors retain raw parser exception messages containing input.**
   Raising EvidenceError inside the ValueError handler leaves the original exception as visible implicit context. Although str(error) and code are redacted, ordinary exception formatting/logging includes the original input-bearing parser message. The generic guard and other conversion handlers use the same unsuppressed pattern.
   Reproduce: set session_meta.payload.timestamp to `/private/path/SECRET_PROMPT`, catch EvidenceError and format it with traceback.format_exception(error). The output includes `ValueError: Invalid isoformat string: '/private/path/SECRET_PROMPT'`. Added failing test: `AdversarialReviewTests.test_review_parser_exception_messages_do_not_leak_input`.

Validation: before reviewer additions, the evidence suite passed 95 tests, the identity suites passed 126 tests, and scripts/check passed (including 321 Rust tests). The five added review tests produce 11 failures, reproducible with:

`PYTHONPATH=evals/lib:evals/tests python3 -m unittest test_codex_identity_evidence.AdversarialReviewTests -v`

Only failing tests and this review were added. No implementation or fixture changes; no live/provisioning checks were run.
