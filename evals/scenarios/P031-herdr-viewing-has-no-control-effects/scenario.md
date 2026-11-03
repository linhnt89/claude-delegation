# P031 Viewing tasks never controls their execution
- Job: J8
- Tier: full
- Judged by: auto
- Status: agreed

## Who
A developer viewing running work without giving the observer execution authority.

## Starting state
An isolated observer setup with controlled workers whose execution and received input can be observed independently. Based on agreed S138 and S140.

## Property
Any sequence of task selection, expansion, collapse, refresh, observer closure and observer restart leaves delegate task execution and worker input unaffected by those viewing actions. Workers may progress naturally, but viewing must not launch, cancel, retry, check, merge or discard work, send worker input, or grant workers additional sandbox or control-socket access.

## Steps
1. Generate seeded viewing-action sequences against the real observer interface while controlled workers run.
2. Compare execution/control records with an independent record of intended delegate actions, allowing natural worker progress.
3. Inspect worker permissions and available control endpoints before and after observation; report the seed and smallest failing action sequence.

## Passes when
- Viewing causes no execution/control action or added worker capability in any generated sequence.

## If this fails
Closing or browsing a dashboard can interfere with my workers or weaken their isolation.

## Failure paths
- Observer disconnection and crashes must have the same lack of control effects as ordinary closure.
- Unchanged visible task state alone is insufficient evidence: inspect worker input and permissions too.
