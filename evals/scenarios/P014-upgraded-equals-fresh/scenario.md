# P014 An upgraded project's kit files equal a fresh init's
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Property
For any earlier kit version, a project set up by it, left untouched and then upgraded has the
same kit files (paths, contents, executable bits) and the same recorded version as a new
project set up by the current `delegate init`.

## If this fails
Old projects drift from new ones: a skill or pane file is stale or missing only in projects
that upgraded.
