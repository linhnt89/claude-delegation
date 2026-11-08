# 036: Codex attribution regression boundaries

- Date: 2026-11-04
- Status: accepted; parked by 040

## Context
Real Codex 0.166.0 conversations sharing a managed app-server reproduced a stale
Herdr pane environment: B's real identity was published on A. The authenticated
check attempt t129 was discarded after safety/oracle review. Later credential-free
probes reproduced the same failure with the genuine integration and a local model
provider. The user approved Codex as a remedy target and approved S146.

## Decision
Keep S145 as the authenticated real-system gate. Add S146 as its credential-free
sibling: only model replies may be stubbed; real TUIs, threads, hooks, serving daemon
and public Herdr identity reporting must remain in the path. Offline test success
and skipped live checks are never feature acceptance.

Separate the pure attribution oracle from live resource handling; break preparation,
evidence readers and final live loop into reviewable waves. Independently review
specs before deep implementation. Preserve check-author family separation via
`checks_by`, verified merge authorship and available capacity; stop when no eligible
family remains rather than overriding the guard or changing worker configuration.

Codex is an approved external remedy target, not blanket approval for a new protocol,
public schema, identity fallback or execution-ownership change. Settle that exact
contract with the user before any such patch. Native Codex observer coverage remains
blocked until attribution is safe.

## Rejected options
- Restarting the shared daemon per pane or bypassing it — avoids the actual problem.
- Synthetic/manual identity publication or unknown-as-success — weakens ownership.
- Passing account credentials to workers — unnecessary for the reproduced bug;
  the exposed temporary authenticated profile was retired with user approval.
- Treating the local-provider probe as authenticated/native/concurrent acceptance —
  those paths require their own gates.

## Consequences
Check implementation can proceed independently of the product remedy, after review
blockers are resolved. Later remedy scheduling requires check-author-independent
capacity; the currently GPT-only deep pool cannot implement after GPT-authored checks.
Concurrency/resume, mixed versions and patched-build delivery remain separately
scoped work, not guarantees supplied by S146.
