# Tests of eval infrastructure

`evals/tests/` holds standard-library `unittest` tests for helpers under `evals/lib/`.
It is not a scenario folder and never produces a user-level product verdict. A pass
here is not an S146 result: the Codex attribution tests are offline, synthetic
oracles with fixture identities and no live Codex, Herdr, credentials or network.

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

These tests do not prove a live product failure or pass, and must not be cited as
S146 acceptance.
