# P020 Reading quota never leaks a secret or changes a login
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Anyone whose logins delegate reads. From: S071, S072. Decision 014.

## Property
For any mix of login files (valid, expired, malformed, missing) and vendor answers (normal,
exhausted, error, timeout, garbage), no `delegate quota` or `delegate run --dry-run` output
contains any token or key from those files, every login file is byte-for-byte unchanged, and
no request goes anywhere but the configured vendor address.

## How it is checked
Seeded random login files and stand-in vendor answers; scans every output and the stand-ins'
request logs. Prints the seed and the smallest failing case.

## If this fails
A quota check exposes the user's credentials or breaks their logins.
