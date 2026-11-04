FINDINGS: 0 high, 0 medium, 1 low

Round 5 review, against SPEC.md (all rounds) and the code on `task/t132`.

Checks:
- `git diff --stat main...HEAD` shows only `evals/lib/codex_identity_prepare.py` and
  `evals/tests/test_codex_identity_prepare.py`. No bytecode or cache files are tracked.
- Both Acceptance unittest commands pass (72 tests, 1 skip). `scripts/check` exits 0.
- Profile TOML matches `fixtures/codex-identity/config.toml` once the root and port are
  substituted. The SSE body matches `local-responses.json`. The start stdout and layout
  checks match `provisioning.json`: `current` is absolute, `release/codex` points to
  the relative `bin/codex`. The hook declaration matches the fixture.
- Round 5 items are done. A `:` in the Herdr parent is rejected before anything is
  written. The source binary may be owned by the current uid or by root and may have
  several hardlinks. Managed-package and hook artifacts still require uid match and nlink=1.
- Probes (no test added, all behaved correctly):
  - A client that resets before accept leaves `invalid` unset.
  - A connection closed before sending any bytes sets `invalid`, as the spec says it should.
  - The 9th simultaneous client sets `invalid`.
  - `herdr_bin` at the pinned live path (owned by the current user, nlink=1, no symlink
    ancestors) passes the source-binary rules.

## Finding 1 (low): the malformed-HTTP test cannot detect a lost `invalid` flag

- **Where:** `evals/tests/test_codex_identity_prepare.py:481` (`test_malformed_http`). The
  same pattern is in `test_authorization_any_casing_empty_and_duplicate` (:474).
- **What is wrong:** every malformed request in the list goes through one shared
  `LocalStub`. The first entry sets `invalid = True`, and `invalid` stays set. After that,
  `assertTrue(self.stub.invalid)` passes no matter what the stub does with the later
  entries. The `exchange(...) == b""` assertion only checks that no response came back.
  So a regression where the stub silently closes a malformed request without setting
  `invalid` would still pass, for every entry after the first. The spec requires that
  unknown or malformed requests set `invalid`.
- **How to reproduce (mutation):** in `LocalStub._parse`, change `self._drop(client, True)`
  to `self._drop(client)` in three branches:
  - the `Connection: upgrade` branch
  - the `len(significant) > 7` branch
  - the `count > 1048576` branch

  Then run
  `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_prepare.py'`.
  All tests still pass. I checked this on a scratch copy.
- **Impact:** a gap in the tests only. The current code sets `invalid` correctly on every
  path I checked, so I added no failing test. Fix: create a fresh stub per subTest, or
  assert that `invalid` is False before each malformed exchange.

No other findings.
