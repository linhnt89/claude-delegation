# 037: Build the bounded Pi/Claude native observer before Codex support

- Date: 2026-11-04
- Status: accepted; amended by 038 (real tasks, no Sheprd patch)

## Context
The goal in decision 035 is a native task dashboard beside the lead conversation,
not a Codex repair project. Real Codex shared-daemon attribution is unsafe. Work
on its regression infrastructure has delayed the native viewer; the user approved
shipping a bounded Pi/Claude slice first and keeping Codex support for later.

## Decision
Proceed with the isolated native GPUI sidebar/dashboard spike for Pi and Claude
leads only. Preserve the lead on the left, read-only task details on the right and
sidebar grouping under the launching lead. `delegate` retains execution ownership.
Use the merged headless adapter and independently observed real Pi/Claude identity;
never infer a lead from checkout name, focus, pane occupancy or a legacy field.

Codex attribution is explicitly unsupported/unknown in this slice. Do not consume
its potentially stale pane identity to assign tasks to a lead. Unsupported identity
must be visible as such, never silently guessed or counted as supported acceptance.
This excludes Codex attribution support, not permission to change worker execution.

Defer the Codex check/resource/driver and remedy wave (t138/t131/t128 and related
preparation) rather than requiring it before t124. Retain already merged t130/t132/
t133 groundwork; close remaining task state without merging unreviewed code. S145
and S146 remain agreed, with no accepted automated live verdict or product fix.

This changes phase sequencing and the bounded native spike's harness scope, not the
ownership/safety contracts of decisions 035/036. No production identity fallback,
protocol/schema migration, permanent history, interactive workers or full property
acceptance is authorized by this reduction. Production instance/attempt identity
(t127) and full P030–P033 acceptance remain separate gates.

## Rejected options
- Keep the native UI blocked on Codex remediation — delays the primary goal despite
  working Pi/Claude identity evidence.
- Treat the reduced spike as all-harness completion or guess Codex ownership — hides
  a known attribution defect and weakens the agreed invariants.
- Merge t138 merely to preserve progress — its code is unreviewed, not accepted.

## Consequences
Next phase scopes t124 against pinned installed GPUI/Herdr sources, maps bounded
Pi/Claude checks to the agreed observer scenarios without weakening them, pins
native read/action hooks and gets an independent plan review before implementation.
Codex work resumes only when explicitly reprioritized with its safety/review gates.
