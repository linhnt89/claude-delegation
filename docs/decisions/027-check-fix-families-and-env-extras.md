# 027: Check fixes take their named checks' families; the extras table

- Date: 2026-11-01
- Status: accepted (proposed by the autopilot lead during plan review; confirmed by the user 2026-11-01)

## Context
Plan review of t085–t098 (2026-11-01) found two things decisions 024 and 025 leave open:
- A `fixes_checks:` task is also an implementer, and the ordinary rule skips every writer
  family of its Acceptance checks. For a fix of S001's check, that rule would skip the same
  families the fix must come from, so no candidate would remain.
- S106 says `.delegate.toml` names extra allowed variables, but no key or table is given.
  A check writer would have to invent the public syntax.

## Decision
- **Fix tasks.** A task with `fixes_checks:` may use only the writer families of the checks it
  names (024). Those named checks are exempt from the ordinary skip rule for that task. The
  skip rule still applies to every other Acceptance check.
- **Extras.** `.delegate.toml` names extra allowed child variables in an `[env]` table:
  `[env]` then `allow = ["EXTRA_ALLOWED"]`. It follows the `[warm]` style and has no other keys.

## Consequences
t086 implements the first rule; t090 implements the second. Checks in t097 use the table as
written here. If the user changes either, t086 and t097 get a round, not a new decision.
