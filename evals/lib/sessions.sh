# Two Claude Code sessions and a plain terminal on one project. Source after common.sh.
# Session A leads from the main checkout, session B from a worktree on branch pane-polish.
SESS_A="3f9a0c1e-7b2d-4e51-9a6f-0c8d2b1e4a71"
SESS_B="8c41d2f0-1a9e-4b73-b5d2-6e0f3a9c7d18"
WT_B="$EVALS_WORK/wt/pane-polish"

# as_session <session id|""> <dir> <command...>: run a check helper (d, write_spec, bg_in) as
# Claude Code would from that session and folder; "" is a plain terminal.
as_session() {
  local sid="$1" dir="$2"; shift 2
  if [ -n "$sid" ]; then export CLAUDE_CODE_SESSION_ID="$sid"; else unset CLAUDE_CODE_SESSION_ID; fi
  DCWD="$dir" "$@"
  unset CLAUDE_CODE_SESSION_ID
}

# bg_in <args...>: `delegate run --background` from $DCWD (default the main checkout).
bg_in() {
  OUT="$(cd "${DCWD:-$PROJ}" && timeout 20 "$DELEGATE" run --background "$@" 2>&1 </dev/null)"; RC=$?
  printf '\n$ (in %s) delegate run --background %s  (exit %s)\n%s\n' "${DCWD:-$PROJ}" "$*" "$RC" "$OUT" >>"$EVALS_WORK/transcript.log"
}

# line_as <session id|""> <dir>: `delegate statusline`, run as Claude Code runs it from that
# session for a user who added it to their status line. Sets LINE (as printed), PLAIN (colours
# stripped), MS and LRC.
line_as() {
  local cmd="delegate statusline" t0 json
  json="$(python3 -c 'import json,sys
d={"cwd":sys.argv[2],"workspace":{"current_dir":sys.argv[2]}}
if sys.argv[1]: d["session_id"]=sys.argv[1]
print(json.dumps(d))' "$1" "$2")"
  t0=$(date +%s%N)
  LINE="$(cd "$2" && printf '%s' "$json" | env -u CLAUDE_CODE_SESSION_ID bash -c "$cmd" 2>&1)"; LRC=$?
  MS=$(( ($(date +%s%N) - t0) / 1000000 ))
  PLAIN="$(printf '%s' "$LINE" | sed 's/\x1b\[[0-9;]*m//g')"
}

# sessions_scene: the starting state of S055/S056. A: t021 running, t022 ready to merge.
# B: t030 running, t032 stalled. Plain terminal: t031 running. Call `flag end` to stop them.
sessions_scene() {
  export DELEGATE_STALL_SECS=3
  simple_workers standard gpt-busy gpt-busy gpt-busy
  stub_worker gpt-busy <<'B'
work() { while [ ! -f "$EVALS_WORK/flags/end" ]; do date >>notes.txt; sleep 0.5; done; }
B
  stub_worker gpt-quiet <<'B'
work() { printf '\n# held\n' >>README.md; wait_flag end 300; }
B
  stub_worker gpt-quick <<'B'
work() { printf '\n\ndef divide_bill(amount, people):\n    return round(amount / people, 2)\n' >>shop/pricing.py; }
B
  (cd "$PROJ" && mkdir -p "$(dirname "$WT_B")" && git worktree add -q -b pane-polish "$WT_B") ||
    fail "setup: could not make session B's worktree"
  as_session "$SESS_A" "$PROJ" write_spec t021 "Project id and vault" "Give each project an id and a locked vault." "true"
  as_session "$SESS_A" "$PROJ" write_spec t022 "Task state in checkouts" "Move task state under the checkout id." "grep -q divide_bill shop/pricing.py"
  as_session "$SESS_B" "$WT_B" write_spec t030 "Status line per session" "Show each session its own tasks." "true"
  as_session "$SESS_B" "$WT_B" write_spec t032 "Pane titles" "Show what each task is for." "true"
  as_session "" "$PROJ" write_spec t031 "Fix wait timeout" "Make wait give up after its timeout." "true"
  as_session "$SESS_A" "$PROJ" bg_in t022 --worker pi:gpt-quick@high
  wait_field 60 t022 phase done || fail "setup: t022 never finished: $OUT"
  as_session "$SESS_A" "$PROJ" bg_in t021
  as_session "$SESS_B" "$WT_B" bg_in t030
  as_session "$SESS_B" "$WT_B" bg_in t032 --worker pi:gpt-quiet@high
  as_session "" "$PROJ" bg_in t031
  local t
  for t in t021 t030 t031 t032; do wait_field 30 "$t" phase implement || fail "setup: $t never started: $OUT"; done
  local end=$(( $(date +%s) + 20 ))
  until [ "$(sj 's["tasks"][0]["stalled"]' t032)" = true ]; do [ "$(date +%s)" -ge "$end" ] && fail "setup: t032 never showed as stalled"; sleep 0.3; done
  [ "$(sj 's["tasks"][0]["stalled"]' t030)" = false ] || fail "setup: t030 shows as stalled though its worker keeps writing"
}
