# 025: Fewer check bugs, found earlier, fixed by one command

- Date: 2026-10-31
- Status: accepted

## Context
Decisions 023 and 024 keep checks apart from the work they judge. They don't make check bugs
rarer or cheaper. Nearly all the cost came from one batch: t064 wrote 15 checks for three
unbuilt features, and t070–t080 then spent five implementer rounds and five fix tasks on them.
Some fixes were one line, yet each still took a full task cycle (10–30 min) while the
implementer waited. The causes:
- check writers guessed outputs and behaviour that `scenario.md` didn't pin down (P024, S083);
- environment leaks into nested runs, whoever wrote the check (806eaa1, t054, t077, and
  autopilot's gate on 2026-10-31);
- brittle checks (S040, S091).

Small check batches (t047, t060, t085) had no such trouble, so the writer model isn't the
cause.

## Decision
- **The lead never edits checks**, small fixes included (023 stands).
- **Pinned contract.** An agreed scenario names what its check observes: the command, the
  output or file it reads, and the exit codes.
- **Small check tasks.** A check task covers at most 5 scenarios. Its spec carries a
  checklist: set up your own data, no assumptions about repo history, deterministic order,
  nothing taken from the ambient environment.
- **Implementers review their checks first.** In the question phase the implementer reads
  its Acceptance checks against their `scenario.md` and flags any it thinks can't pass. It
  only reports; the lead judges, and a flagged check is fixed before the round starts.
- **`delegate fix-check <S…> --from <task> --reason "…"`.** The lead judges a failure to be a
  check bug and gives one reason. The command builds the fix spec from:
  - the scenario;
  - the failing output in `CHECKS.txt`;
  - the implementer's "Check problems" REPORT section, marked as coming from the work being
    judged;
  - the reason.

  It fills in `fixes_checks:` and Acceptance, then starts the writer family (024). That
  worker may answer "the check is right" and change nothing; the failure then goes back to
  the implementer as a product bug.
- **Fixes reach running tasks.** A task's next round includes check fixes already merged
  into its base branch; the lead doesn't merge them in by hand.
- **Clean child environment.** Gates, acceptance commands and checks run with an allowlisted
  environment, built in one place. Unit tests don't read ambient `DELEGATE_*` variables.
- **Autopilot may fix checks.** Its lead may run `fix-check` (this amends 019's "fix tasks
  only for breakage it caused"). It parks the failure only when it can't tell a check bug
  from a product bug.

## Rejected options
- The lead fixing small check bugs itself (a size-limited lane with writer-family review) —
  a fix is made while the check fails against one implementation, after the lead has read
  that diff, which is when bending the check is most tempting; it would undo 023 where 023
  matters most.
- A fake of the correct product to show each check can pass — not realistic for stateful
  features (processes, locks, sandboxing), where the fake approaches a second implementation.
- Writing check-fix specs by hand — the lead's judgment fits in one sentence, and the rest
  already exists.

## Consequences
Scenarios take longer to agree, since each pins its contract. Check bugs surface in the
question phase rather than after a failed round. A check fix costs one command plus the
writer family's round. The REPORT template gains a fixed "Check problems" section.
