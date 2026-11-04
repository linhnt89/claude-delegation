# A stand-in Herdr endpoint for checks. Source after common.sh.
#
#   herdr_up                      # start it; sets HSOCK, HPANES, HLOG, HTOKENS
#   herdr_panes <<'EOF' ... EOF   # the panes, as JSON (see herdr_standin.py)
#   in_pane <pane> <command...>   # run a helper (d, write_spec...) as from that Herdr pane
#   herdr_down                    # stop it (also on EXIT through herdr_trap)
#
# The socket lives under a short private /tmp folder: UNIX socket paths are limited to
# about 100 bytes and $EVALS_WORK can be longer.

herdr_up() {
  HDIR="$(mktemp -d /tmp/hs.XXXXXX)" || fail "setup: no temp folder for the Herdr stand-in"
  chmod 700 "$HDIR"
  export HSOCK="$HDIR/herdr.sock" HPANES="$EVALS_WORK/herdr-panes.json" HLOG="$EVALS_WORK/herdr-requests.jsonl"
  export HTOKENS="$HPANES.tokens.json"
  [ -f "$HPANES" ] || echo '{}' >"$HPANES"
  : >"$HLOG"
  python3 "$EVALS_LIB/herdr_standin.py" "$HSOCK" "$HPANES" "$HLOG" >"$EVALS_WORK/herdr-standin.log" 2>&1 &
  HPID=$!
  local i
  for i in $(seq 50); do [ -S "$HSOCK" ] && return 0; sleep 0.1; done
  echo "setup: the Herdr stand-in never listened (see herdr-standin.log)"; exit 3
}

herdr_down() {
  [ -n "${HPID:-}" ] && kill "$HPID" 2>/dev/null && wait "$HPID" 2>/dev/null
  [ -n "${HDIR:-}" ] && rm -rf "$HDIR"
  true
}

# herdr_panes: replace the panes from stdin (atomically: the stand-in re-reads per request).
herdr_panes() { cat >"$HPANES.tmp" && mv "$HPANES.tmp" "$HPANES"; }

# in_pane <pane> [--claude <session id>] <command...>: run a helper (d, write_spec...) as a
# lead in that Herdr pane would; --claude also sets CLAUDE_CODE_SESSION_ID, as Claude Code does.
# Helpers set globals (OUT, RC), so this exports and unsets instead of using a subshell.
in_pane() {
  local pane="$1" claude="" rc; shift
  if [ "${1:-}" = "--claude" ]; then claude="$2"; shift 2; fi
  export HERDR_ENV=1 HERDR_SOCKET_PATH="$HSOCK" HERDR_PANE_ID="$pane" \
    HERDR_WORKSPACE_ID="${pane%%:*}" HERDR_TAB_ID="${pane%%:*}:t1" HERDR_BIN_PATH=/nonexistent/herdr
  if [ -n "$claude" ]; then export CLAUDE_CODE_SESSION_ID="$claude"; fi
  "$@"; rc=$?
  unset HERDR_ENV HERDR_SOCKET_PATH HERDR_PANE_ID HERDR_WORKSPACE_ID HERDR_TAB_ID HERDR_BIN_PATH CLAUDE_CODE_SESSION_ID
  return $rc
}

# herdr_requests [method]: the recorded requests (one JSON per line), optionally one method.
herdr_requests() {
  if [ -n "${1:-}" ]; then
    python3 -c 'import json,sys
for l in open(sys.argv[1]):
  r=json.loads(l)
  if r["method"]==sys.argv[2]: print(l,end="")' "$HLOG" "$1"
  else cat "$HLOG"; fi
}
