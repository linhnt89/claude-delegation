# 011: The vault syncs only when the hidden set is changed or run

- Date: 2026-10-27
- Status: accepted

## Context
Decision 010 made every command except `where`, `statusline` and help sync the vault with its
remote. With a real remote (GitHub: fetch about 0.9 s, push about 0.8 s) and several Claude Code
sessions whose panes poll `delegate status` every few seconds, commands queued on the vault
lock behind each other's network calls: `list` took 10–22 s, the status line 3–6 s, and
`run --background` could not start (S059). Bounding each command's sync (t035, four rounds)
moved the problem into budgets, detached syncs and lock deadlines, and review still found
hangs. The vault holds only the hidden eval set, and only two things use that set: changing it
and running it.

## Decision
- The vault syncs (commit hand edits, fetch and merge, push) only at:
  - `delegate setup --vault <url>`: connect and pull;
  - `delegate hidden add`: commit and push;
  - `evals/run`, when it will run hidden scenarios: commit hand edits, pull, push, then run.
    It goes on with the local copy, saying so, when the remote doesn't answer.
- Every other command and the status line never touch the network or wait on the vault lock.
- `delegate list` names hidden changes that aren't on the remote yet (uncommitted hand edits,
  unpushed commits), worked out locally from the vault, with no network.
- Network calls have a hard time limit: the transport is killed when it runs out, even if it
  ignores SIGTERM. No process holds the vault lock while it waits on the network.
- Inside the sandbox nothing syncs (decision 010's `DELEGATE_SANDBOXED=1` rule stands).
- Decision 010 otherwise stands: folder naming, merging deletions, conflicts kept on
  `refs/delegate/theirs`, never reset or force-push.

## Rejected options
- Every command syncs within a time budget, finishing in a detached process (t035) — complex,
  network traffic on every pane poll, and still found hanging in review.
- Read-only views skip the sync but other commands keep it — `run`, `merge` and the rest gain
  nothing from it and still pay for it.

## Consequences
Ordinary commands are local and quick whatever the remote does. A hand edit to a hidden
scenario reaches the remote at the next `evals/run` or `hidden add` on that machine; until then
`delegate list` says it isn't pushed. A machine that never runs the hidden set never pulls it,
which is fine: nothing there uses it. Supersedes decision 010's sync rule (its "every command
syncs" bullet and "one command on each machine" sentence) and decision 008's "pushed by the next
`delegate` command".
