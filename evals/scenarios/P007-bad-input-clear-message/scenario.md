# P007 Bad input gets a clear message, never a crash or mess
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Anyone using `delegate`, directly or through Claude as lead. From: S003, S014.

## Property
For any malformed spec, `workers.toml`, `.delegate.toml` or `--worker` value, every command exits with a message that names the problem, never a panic, and leaves no half-made task state.

## How it is checked
Generated mutations of valid files (missing sections, wrong types, unknown harness, bad effort, broken TOML) fed to each command. Seeded; prints the seed and the smallest failing case.

## If this fails
Users hit crashes or cryptic errors and have to clean up by hand.
