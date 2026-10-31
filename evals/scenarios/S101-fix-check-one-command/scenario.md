# S101 The lead fixes a check bug with one command and one reason
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, reviewing t003's failed round and judging that S001's check is wrong, not the
product. Decision 025.

## Starting state
t001 wrote S001's check and was merged with a DeepSeek-family `Worker:` trailer. t003 (an
implementer, GPT family) finished a round in which `evals/run S001` failed: its `.task/CHECKS.txt`
holds the failing output and its `.task/REPORT.md` has a `## Check problems` section naming
S001. The implement tier lists a GPT-family model and a DeepSeek-family model.

## Steps
1. `delegate fix-check S001 --from t003 --reason "the check reads stdout but the scenario says stderr"`

## Contract
- Command: `delegate fix-check <S…> --from <task> --reason "<text>"`; exit 0 on success.
- Output names the new task's id; `delegate list` shows that task running on the DeepSeek family.
- The new task's `.task/SPEC.md` (in its worktree) contains: S001's `scenario.md` text, the
  failing output from t003's `CHECKS.txt`, t003's `## Check problems` text under a heading or
  line that says it comes from the work being judged, the reason verbatim, a `fixes_checks: S001`
  line, and `evals/run S001` in Acceptance.

## Passes when
- One command starts the fix on the family that wrote the check; the lead writes no spec.
- The fix worker can see what failed and why the lead thinks the check is wrong, and is told
  which part came from the implementer.

## If this fails
Every check bug costs a hand-written spec and a full task cycle, or the fix goes to a family
that didn't write the check.

## Failure paths
- See S102.
