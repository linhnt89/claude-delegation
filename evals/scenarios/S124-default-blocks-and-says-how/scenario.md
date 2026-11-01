# S124 Without the setting, the block says how to unblock
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The same user as S123, before opting in. Decision 029.

## Starting state
As S123, but workers.toml has no `independence` key.

## Steps
1. `delegate run t003`.

## Passes when
- No worker starts. The message names the skipped families and S001's checks, and says to set
  `independence = "model"` under `[defaults]` in `~/.config/delegate/workers.toml`.
- Exit code is not 0, and no task round is recorded as a product failure.

## If this fails
The user hits a dead end with no idea that one setting would let the work go on.

## Failure paths
- `independence = "bogus"` is refused when workers.toml is read, naming the key and its two
  allowed values.
