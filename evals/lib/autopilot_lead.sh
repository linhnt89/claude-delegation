# The stand-in headless lead: sourced inside the `claude` script the autopilot driver runs.
# The per-check behaviour file ($EVALS_WORK/autopilot/lead.sh) only sets LEAD_* variables;
# wave() below runs the next ready tasks of the plan as one wave, merges them, and parks any
# task whose worker asks a question. It never touches the user's checkout: it runs in the
# autopilot worktree, where `delegate` is the only thing that changes anything.

lead_log() { printf '%s\n' "$*" >>"$EVALS_WORK/autopilot/lead.log"; }

lead_merged() {
  git -C "$PWD" log --format=%s 2>/dev/null |
    sed -n 's/^\(t[0-9][0-9][0-9]\): .*/\1/p' | sort -u
}
lead_is_merged() { lead_merged | grep -qx "$1"; }
lead_is_parked() { [ -f PLAN.md ] && grep -q "^- $1:" PLAN.md; }

lead_deps_of() {
  local pair
  for pair in ${LEAD_DEPS:-}; do
    case "$pair" in "$1="*) printf '%s\n' "${pair#*=}" ;; esac
  done
}

lead_questions_text() {
  local home
  home="$(delegate where 2>/dev/null | awk '$1 == "tasks" {sub(/^tasks[ \t]+/, ""); print; exit}')"
  [ -n "$home" ] && sed -n '2p' "$home/$1/QUESTIONS.md" 2>/dev/null
}

# The stand-in lead never decides whether the product's lock lets it in: it always asks the
# real CLI, so a refused merge or run is the product's verdict, not the stand-in's.
lead_run() {
  local id="$1" out rc
  out="$(delegate run "$id" 2>&1)"; rc=$?
  LEAD_OUT="$out"; LEAD_RC=$rc
  { printf '\n$ lead run %s (exit %s)\n' "$id" "$rc"; printf '%s\n' "$out"; } \
    >>"$EVALS_WORK/autopilot/lead.log"
  return 0
}

lead_asked() { printf '%s' "$LEAD_OUT" | grep -q 'has questions before starting'; }

lead_merge() {
  local id="$1" out rc
  out="$(delegate merge "$id" 2>&1)"; rc=$?
  { printf '\n$ lead merge %s (exit %s)\n' "$id" "$rc"; printf '%s\n' "$out"; } \
    >>"$EVALS_WORK/autopilot/lead.log"
  return 0
}

lead_park() {
  local id="$1" question="$2"
  [ -f PLAN.md ] || printf '# Plan\n' >PLAN.md
  grep -qx '## Waiting for you' PLAN.md || printf '\n## Waiting for you\n' >>PLAN.md
  printf -- '- %s: %s\n' "$id" "$question" >>PLAN.md
  # The handoff is committed so the next merge is not refused for a dirty worktree.
  git -C "$PWD" add PLAN.md >/dev/null 2>&1
  git -C "$PWD" commit -qm "Plan: park $id" >/dev/null 2>&1 || true
  lead_log "parked $id"
}

# --- the lead's stream, and what the checks watching it do ----------------------------------
# A headless lead prints stream-json on stdout; the driver records it in the wave's log and a
# view's step adapter (the one Claude workers use) turns it into steps. The stand-in lead prints
# the same events, so a check can watch a running wave. Arguments are one line of text each
# (no newlines); quotes and backslashes are escaped.
lead_stream_text() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }

lead_stream_event() { # <tool name> <input key> <input value>
  LEAD_STREAM_N=$(( ${LEAD_STREAM_N:-0} + 1 ))
  printf '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"lead-step-%s","name":"%s","input":{"%s":"%s"}}]}}\n' \
    "$LEAD_STREAM_N" "$1" "$2" "$(lead_stream_text "$3")"
}

# lead_stream_read <path>: the lead reads a file.
lead_stream_read() { lead_stream_event Read file_path "$1"; }

# lead_stream_run <command>: the lead runs a command.
lead_stream_run() { lead_stream_event Bash command "$1"; }

# lead_stream_say <text>: the lead tells the user something.
lead_stream_say() {
  LEAD_STREAM_N=$(( ${LEAD_STREAM_N:-0} + 1 ))
  printf '{"type":"assistant","message":{"content":[{"type":"text","text":"%s"}]}}\n' \
    "$(lead_stream_text "$1")"
}

# lead_flag <name>: tell the check this wave got here ($EVALS_WORK/flags/<name>).
lead_flag() {
  mkdir -p "$EVALS_WORK/flags"
  : >"$EVALS_WORK/flags/$1"
}

# lead_wait <flag> [seconds]: hold the wave until the check writes that flag.
lead_wait() {
  local i
  for i in $(seq $(( ${2:-120} * 5 ))); do
    [ -f "$EVALS_WORK/flags/$1" ] && return 0
    sleep 0.2
  done
  return 1
}

wave() {
  local id dep ok ready="" n=0
  for id in ${LEAD_ORDER:-}; do
    lead_is_merged "$id" && continue
    lead_is_parked "$id" && continue
    ok=1
    for dep in $(lead_deps_of "$id"); do
      lead_is_merged "$dep" || { ok=0; break; }
    done
    [ "$ok" = 1 ] || continue
    ready="$ready $id"
  done
  for id in $ready; do
    [ "$n" -ge "${LEAD_WAVE_SIZE:-1}" ] && break
    n=$((n + 1))
    lead_run "$id"
    if lead_asked; then
      lead_park "$id" "$(lead_questions_text "$id")"
      continue
    fi
    lead_merge "$id" || return 0
  done
}
