# Herdr observer prototype: plan review and scope correction

2026-11-04. Independent reviewer: Pi, GitHub Copilot Claude Opus 5.5, high effort.
Selected explicitly for this review at the user's request; workers.toml unchanged.

The initial t123 draft combined launch attribution, status adaptation, a native GPUI
patch, smoke tests, permanent properties and native evidence. Review blocked it on
scope, contradictory live-identity test requirements and cold GPUI build assumptions.
The report also identified absent task/attempt markers in status JSON, inherited
worker reporting hints, legacy-owner ambiguity, whole-feed failures, uncommitted
context and premature property checks. Its header severity counts did not match
its numbered findings; conclusions here are based on the individual findings.

## Revised execution order
1. Commit the approved decision/research/eval context on main before any worker starts.
   t122's definition phase is complete; automated product verdicts remain outstanding.
2. t123: headless launch-attribution sidecar and status adapter only, with documented
   executable/read contract and black-box implementation smoke tests. Explicit API-shaped
   fixtures establish parsing, not live harness integration. No native build in acceptance.
3. t124: isolated pinned penso/herdr-gpui patch consuming that contract. Scope/spec only
   after adapter review. Patch applicability and builds are explicit opt-in lead gates,
   not a cold fetch/build hidden inside a short sandboxed automatic check.
4. t125: independent P030–P033 checks after their observation boundary is pinned and
   approved. Set checks_by to this task for subsequent implementation rounds; do not
   pretend that partial spike coverage proves the complete agreed properties.
5. Lead runs private live/native demonstrations. Store results outside task branches
   in private task-review state before merge, with a sanitized durable summary on main
   after merge. Required evidence gaps block declaring the full prototype demonstrated.

## Explicit limitations and safeguards
- Sidecar attribution is sole prototype origin authority; legacy session fields never
  fill unknown identity. Worker launch strips reporting hints; this is not socket security.
- Snapshot activity is replaced wholesale, not merged by reused step ids. Status lacks
  reliable per-fallback-attempt/instance markers. Wrapped id reuse is covered; unwrapped
  offline reuse and full attempt fencing remain gaps requiring production follow-up.
- Failed all-task reads stay stale/unavailable; registered tasks can be read individually
  to isolate corruption. This does not claim complete discovery of unregistered tasks.
- No production behavior changes in this spike. Decisions 009/031 remain authoritative
  for production; a later decision must reconcile ownership integration with 035.
- HUMAN/no-check eval output is not a product pass and is not an automatic merge gate.
- Add prototypes/ to the repository layout only when that directory actually lands.

## Re-review of the narrowed t123 spec

The same user-authorized Copilot/Pi Opus 5.5 reviewer reported **0 high, 8 medium,
5 low** findings. No worker had started at re-review. The spec has now been revised for these findings;
final plan review must validate the corrections before execution:
- Size/runway: keep the narrowed slice long or split attribution from snapshot adaptation.
- Status CLI housekeeping: it can write checkout registry/migration metadata despite
  read-only task collection. Pin task-level no-control assertions, not whole-data purity.
- Namespace: use delegate's shared checkout identity, deduplicate linked worktrees and
  test equal task ids in distinct checkouts.
- Crash-safe creation: mark origin pending before creation, publish atomically after;
  pending reads unknown. Declare all unwrapped id reuse a gap, not just offline reuse.
- Every spike run/retry must use scrubbed setup; document unsupported direct runs and
  test a later capture from the same pane. Do not promise real credentialed workers yet.
- Pin fixture field expectations with cited upstream paths/commit; fixture checks cannot
  claim live schema/harness conformance from a network fetch unavailable in acceptance.
- Live identity capture is a gate immediately after t123, before native GPUI scoping.
- P030/P031 need the actual GPUI sidebar/view action boundary, not merely snapshot CLI.
  Put test-only GPUI read/action hooks in t124 and scope t125 only once those are pinned.
- Also pin source-built delegate selection and isolated environment for smoke tests,
  degraded missing-task semantics/step bounds, and full reference-kind identity keys.
- Preserve the explicit main-branch context-commit prerequisite before task worktree creation.

## Corrections prepared for execution
- Keep t123 as one headless slice, explicitly size long (the review's permitted alternative).
- Add task-level purity assertions, explicit shared checkout UUIDs and linked-worktree tests,
  pending-origin creation transactions, all-unwrapped-reuse limitations, all-runs-via-wrapper,
  offline cited fixture contracts, source-built delegate selection and uniform five-step bounds.
- Use full source/agent/reference-kind/value identity keys. Put a private live identity gate
  immediately after t123, and actual native view/action automation in t124 before t125.
- Production decision reconciliation remains deferred because this spike changes no delegate
  owner fields or production protocols. Reports must explicitly list that doc conflict.

## Final pre-execution fixes

A further review found duplicate creation could clear an existing task's valid origin.
The spec now requires a successful absence preflight under its lock, refusing duplicate
or failed reads without modifying attribution; creation/publication still uses pending.
It also defines explicit discard/observed-absence retirement, honest unknown truncation
at the upstream five-step cap, idle-only file comparisons plus status-dispatch auditing,
private UNIX-socket fixture transport and a deterministic test-only crash hook.
Production instance/attempt/decision work is tracked separately in PLAN, not silently
claimed by the prototype. Before building the headless slice, a private live probe
confirmed actual Pi TUI and Claude startup reporting; Codex remains unverified (see
herdr-observer-live-identity.md). No personal credentials/configuration were used.

Remaining review follow-up is verification of these contracts against implementation;
no implementation has been reviewed yet. The user authorized proceeding and merging
only green tasks. Main-branch context commit remains mandatory before worker startup.

Review remains a planning check, not runtime feasibility evidence or user acceptance.
