# P013 A second upgrade changes nothing
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Property
For any project, running `delegate upgrade` again right after an upgrade leaves every file
unchanged and says the project is up to date (still naming any kept edits).

## If this fails
Upgrade churns files on every run, so its diff can't be trusted.
