---
name: distill
description: End a discussion or brainstorm by saving only what lasts — decisions and plan changes — into the repo so other models and future sessions can act on it. Use when the user says /distill, "wrap up the discussion", "write down what we decided", or before delegating work that came out of a discussion.
---

# Distill a discussion

Most discussion should leave **no** permanent text. Save only what the code can't say.

## 1. List what was settled
From this conversation, list candidates as one line each:
- **Decisions** (we chose X over Y, and why)
- **Plan changes** (new tasks, reordering, dropped work)
- **Open questions** still unresolved
- **Durable facts** (a new command, convention, or "never do X")

Drop everything else: options we only explored, abandoned ideas (unless rejected *on
purpose*, which goes in a decision), and anything the code already shows.

Show the list to the user and let them correct it before writing.

## 2. Write it
- Decision → new `docs/decisions/NNN-<slug>.md` from `000-template.md`, status `accepted`
  (or `proposed` if the user hasn't committed). If it replaces an older decision, set the
  old one's Status to `superseded by NNN` and change nothing else in it.
- Plan change → edit `PLAN.md`. Remove tasks marked `done` while there.
- Open question → `PLAN.md` "Open questions", with what it blocks.
- Durable fact → one line in `AGENTS.md`, or the module README it belongs to.
  If `AGENTS.md` would go over ~150 lines, cut something stale first.

## 3. Check
- Nothing written contradicts the code; if it does, the code wins unless the decision
  is to change the code, in which case add a task.
- No transcript excerpts or summaries of the conversation were saved.
- Report what was written, one line per file.
