# Tests of eval infrastructure

`evals/tests/` holds standard-library `unittest` tests for helpers under `evals/lib/`.
It is not a scenario folder and never produces a user-level product verdict. A pass
here is not an S146 result: the Codex attribution tests use synthetic/fixture
inputs, no live Codex/Herdr or credentials, and no external network. Preparation
stub tests exercise private loopback sockets.

Run them with:

```
python3 -B -m unittest discover -s evals/tests -p 'test_*.py' -v
```

`scripts/check` runs the same discovery so these tests keep running after a merge.

- `test_codex_identity_oracle.py` builds valid traces with non-fixture pane ids,
  then mutates one aspect per regression: pane binding, grace/dwell/gap boundaries,
  failed reads, wrong qualified identities, wrong-pane publication, daemon
  continuity and precedence. Seeded cases print seed and inputs on failure.
- `test_codex_identity_oracle_review.py` is independent adversarial evidence for
  qualified wrong-pane publication; it also uses synthetic traces only.

- `test_codex_identity_evidence.py` validates real-format redacted rollout/PID/proc
  fixtures and adversarial binding, complete output, integer and timestamp cases.
  It only parses snapshots; trusted acquisition and live continuity are separate.
- `test_codex_identity_prepare.py` checks fresh profiles, environment isolation,
  pinned package/hook validation, auth-entry rejection and bounded local Responses
  HTTP/SSE behavior. Timers use an injected clock; sockets are loopback only.

These tests do not prove a live product failure or pass, and must not be cited as
S146 acceptance.
