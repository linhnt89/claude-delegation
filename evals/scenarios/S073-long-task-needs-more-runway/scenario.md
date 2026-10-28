# S073 A long task goes to an account that can last
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, who marked a spec `size: long`. Decision 014.

## Starting state
`[quota.runway]` sets small 30 minutes, medium 1 hour, long 3 hours. The first candidate's
account has 2 hours of runway at its current pace; the second's lasts through its reset.

## Steps
1. `delegate run t001 --dry-run` with `size: long`; again with `size: small`; again with no size.

## Passes when
- `long`: the first candidate is skipped because 2 hours is less than the 3 a long task needs.
- `small` and no size (medium): the first candidate is used.
- A bad `size:` value is refused with the values that work.

## If this fails
A long task starts on an account that can't see it through, or a small one is kept off an
account with plenty for it.

## Failure paths
- None beyond the above.
