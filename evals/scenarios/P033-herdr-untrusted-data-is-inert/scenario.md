# P033 Untrusted task data stays inert and failures stay local
- Job: J8
- Tier: full
- Judged by: auto
- Status: agreed

## Who
A developer viewing activity emitted by unreliable workers.

## Starting state
An isolated observer with a healthy task and a task-data source capable of supplying malformed records, oversized strings, control sequences and shell-looking text. Based on agreed S144.

## Property
Changing one task's text or activity to untrusted or malformed data must not execute that text, trigger worker controls, alter another task's attribution or results, or prevent healthy tasks from being inspected. Unsupported or bounded-away detail is explicitly unavailable or truncated. Once valid data is restored, successful refresh recovers the affected task without restarting its worker.

## Steps
1. Generate seeded hostile task text and malformed activity through the real observer's task-data boundary, not imported product internals.
2. Inspect the public observer output, healthy-task visibility and independent execution/control records.
3. Restore valid data and refresh; report the seed and smallest failing input.

## Passes when
- Every generated input preserves inert rendering and healthy-task isolation, with explicit truncation or unavailable detail where needed.
- The affected task recovers on a successful valid refresh.

## If this fails
One worker's output can disrupt my workspace, impersonate another task or execute unintended actions.

## Failure paths
- A source-wide connection failure may mark all tasks unavailable; one malformed task record must not be treated as a source-wide empty task set.
- Concrete resource limits and the automation interface must be specified before implementing this check; no arbitrary performance threshold is approved here.
