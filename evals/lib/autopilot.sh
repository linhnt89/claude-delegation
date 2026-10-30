# Shared setup and helpers for the autopilot checks (S083-S091, P022-P024).
#
#   . "$EVALS_LIB/common.sh"
#   . "$EVALS_LIB/autopilot.sh"
#   ap_setup                     # $PROJ after delegate init, stand-in workers and lead
#   ap_specs t001 t002           # ready specs, acceptance checks run in the worktree
#   ap_write_plan "true" "t001 t002" "t002=t001"
#   ap_setup_lead 1 "t001 t002" "t002=t001"
#   ap_start                     # FAILs (exit 1) when the feature is missing
#   ap_wait_stopped 90 || fail ...
#
# The stand-in `claude` runs in the autopilot worktree and plays one wave from the LEAD_*
# variables in $EVALS_WORK/autopilot/lead.sh (see autopilot_lead.sh). One notification
# recorder is installed at $EVALS_WORK/bin/record-notify.

AP_WORK="$EVALS_WORK/autopilot"

# ap_workers [notify command]: stand-in pi workers, plus the optional autopilot notifier.
ap_workers() {
  simple_workers standard gpt-good gpt-good gpt-good
  if [ -n "${1:-}" ]; then
    printf '\n[autopilot]\nnotify = "%s"\n' "$1" >>"$HOME/.config/delegate/workers.toml"
  fi
}

# ap_notify_recorder: a notify command that appends the delivered line and env to notify.log.
ap_notify_recorder() {
  cat >"$EVALS_WORK/bin/record-notify" <<EOF
#!/usr/bin/env bash
{ printf 'MESSAGE=%s\n' "\$DELEGATE_MESSAGE"
  printf 'TITLE=%s\n' "\$DELEGATE_TITLE"
  printf 'PROJECT=%s\n' "\$DELEGATE_PROJECT"
  printf 'REPORT=%s\n' "\$DELEGATE_REPORT"
} >>"$EVALS_WORK/notify.log"
EOF
  chmod +x "$EVALS_WORK/bin/record-notify"
  echo "$EVALS_WORK/bin/record-notify"
}

ap_worker() {
  stub_worker "${1:-gpt-good}" <<'B'
task_id() { sed -n '1s/^# Task \([^:]*\):.*/\1/p' .task/SPEC.md 2>/dev/null; }
mode() { cat "$EVALS_WORK/flags/$1" 2>/dev/null || echo pass; }
ask() {
  id="$(task_id)"; m="$(mode "$id")"
  if [ "$m" = questions ]; then
    printf 'QUESTIONS\n1. Should %s round down or to the nearest cent?\n' "$id" >.task/QUESTIONS.md
  else
    printf 'READY\n' >.task/QUESTIONS.md
  fi
}
work() {
  id="$(task_id)"; m="$(mode "$id")"
  case "$m" in
    questions) : ;;
    crash) exit 1 ;;
    fail) printf '# %s wrong\n' "$id" >"shop/${id}.py" ;;
    broken) printf '# %s ok\n' "$id" >"shop/${id}.py"; printf 'broken\n' >shop/broken.txt ;;
    slow) mkdir -p "$EVALS_WORK/flags"; printf '%s\n' "$id" >"$EVALS_WORK/flags/$id-started"; wait_flag "$id-release" 300 || true; printf '# %s ok\n' "$id" >"shop/${id}.py" ;;
    *) printf '# %s ok\n' "$id" >"shop/${id}.py" ;;
  esac
}
B
}

# ap_specs <id>...: specs whose acceptance passes only when the stand-in wrote "# <id> ok".
ap_specs() {
  local id
  for id in "$@"; do
    write_spec "$id" "Do $id" "Make $id pass." \
      "grep -qx '# $id ok' shop/$id.py"
  done
}

# ap_write_plan <gate> <ids...>: PLAN.md with a mandate for exactly these goal ids.
# LEAD_GOAL overrides the Goal ids (default: all of them); LEAD_MANDATE_EXTRA adds
# mandate lines after `Needs the user:` (e.g. `- Max rounds: 2`).
ap_write_plan() {
  local gate="$1"; shift
  local ids="$*" id pair dep line="" goal
  goal="${LEAD_GOAL:-$ids}"
  for id in $goal; do line="$line${line:+, }$id"; done
  {
    printf '# Plan\n\n## Mandate\n'
    printf -- '- Goal: %s\n' "$line"
    printf -- '- Gate: `%s`\n' "$gate"
    printf -- '- Merge without asking when: the checks pass\n'
    printf -- '- Needs the user: new decisions, scope changes\n'
    [ -n "${LEAD_MANDATE_EXTRA:-}" ] && printf '%s\n' "$LEAD_MANDATE_EXTRA"
    printf '\n## Tasks\n| wave | id | task | status | depends on |\n'
    printf '|------|----|------|--------|------------|\n'
    for id in $ids; do
      dep="–"
      for pair in ${LEAD_DEPS:-}; do
        case "$pair" in "$id="*) dep="${pair#*=}" ;; esac
      done
      printf '| 1 | %s | do %s | ready | %s |\n' "$id" "$id" "$dep"
    done
  } >"$PROJ/PLAN.md"
  # The autopilot worktree starts at the base branch's HEAD, so the mandate has to be committed.
  (cd "$PROJ" && git add PLAN.md && git commit -qm "Plan with the autopilot mandate" >/dev/null)
}

# ap_setup_lead <wave size> <ids> [deps id=dep]: install the stand-in `claude`.
ap_setup_lead() {
  local size="$1" order="$2" deps="${3:-}"
  LEAD_DEPS="$deps"
  mkdir -p "$AP_WORK" "$EVALS_WORK/bin"
  cat >"$AP_WORK/lead.sh" <<EOF
LEAD_WAVE_SIZE=$size
LEAD_ORDER="$order"
LEAD_DEPS="$deps"
EOF
  {
    printf '#!/usr/bin/env bash\nexport EVALS_WORK=%q\nexport EVALS_LIB=%q\n' \
      "$EVALS_WORK" "$EVALS_LIB"
    cat <<'STUB'
# Stand-in headless lead for the autopilot driver (decision 019).
printf '%s\n' "${DELEGATE_AUTOPILOT_WAVE:-0}" >>"$EVALS_WORK/autopilot/waves"
# The run id and lead process let checks test an orphaned lead after the driver dies.
printf '%s\n' "${DELEGATE_AUTOPILOT:-}" >"$EVALS_WORK/autopilot/run-id"
printf '%s\n' "$$" >"$EVALS_WORK/autopilot/lead.pid"
. "$EVALS_LIB/autopilot_lead.sh"
. "$EVALS_WORK/autopilot/lead.sh"
if declare -F wave >/dev/null; then wave || true; fi
printf '{"type":"result","subtype":"success","is_error":false,"result":"stand-in wave %s"}\n' \
  "${DELEGATE_AUTOPILOT_WAVE:-0}"
STUB
  } >"$EVALS_WORK/bin/claude"
  chmod +x "$EVALS_WORK/bin/claude"
}

# ap_setup: a project after init with workers, the stand-in lead and an empty work folder.
ap_setup() {
  setup_project
  mkdir -p "$HOME/.cache/stub/flags"
  if [ -d "$EVALS_WORK/flags" ] && [ ! -L "$EVALS_WORK/flags" ]; then
    cp -a "$EVALS_WORK/flags/." "$HOME/.cache/stub/flags/"
  fi
  rm -rf "$EVALS_WORK/flags"
  ln -s "$HOME/.cache/stub/flags" "$EVALS_WORK/flags"
  rm -rf "$AP_WORK"
  mkdir -p "$AP_WORK"
  AP_NOTIFY="$(ap_notify_recorder)"
  ap_workers "$AP_NOTIFY"
  ap_worker gpt-good
}

ap_reset_work() {
  rm -rf "$AP_WORK"
  mkdir -p "$AP_WORK"
  rm -f "$EVALS_WORK/notify.log"
  rm -rf "$EVALS_WORK/flags"
}

# --- delegate autopilot, through the real CLI ---

ap_start() {
  d autopilot start "$@"
  [ "$RC" -eq 0 ] || fail "delegate autopilot start $*: $OUT"
}

ap_stop() {
  d autopilot stop
  [ "$RC" -eq 0 ] || fail "delegate autopilot stop: $OUT"
}

ap_json() { (cd "$PROJ" 2>/dev/null && "$DELEGATE" autopilot status --json 2>/dev/null); }

# ap_get <field>: the JSON field, "" when absent; never an error (the feature may be missing).
ap_get() {
  ap_json | python3 -c 'import json, sys
try: d = json.load(sys.stdin)
except Exception: sys.exit(0)
v = d.get(sys.argv[1]) if isinstance(d, dict) else None
if v is None: print("")
elif isinstance(v, bool): print("true" if v else "false")
else: print(v)' "$1"
}

ap_wait_running() {
  local end=$(( $(date +%s) + ${1:-30} ))
  until [ "$(ap_get running)" = true ]; do
    [ "$(date +%s)" -ge "$end" ] && return 1
    sleep 0.2
  done
}

ap_wait_stopped() {
  local end=$(( $(date +%s) + ${1:-90} ))
  while :; do
    if [ "$(ap_get running)" = false ] && [ -n "$(ap_get stop_reason)" ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$end" ]; then
      ap_json >"$AP_WORK/status.timeout" 2>&1
      return 1
    fi
    sleep 0.2
  done
}

ap_status_fail() { fail "delegate autopilot: $1 (status: $(ap_json 2>/dev/null | tr '\n' ' ' | cut -c1-300))"; }

ap_kill_driver() {
  local pid
  pid="$(ap_get pid)"
  [ -n "$pid" ] || return 1
  kill -KILL "$pid" 2>/dev/null
}

ap_merged() {
  (cd "$PROJ" 2>/dev/null && git log --format=%s autopilot 2>/dev/null |
    sed -n 's/^\(t[0-9][0-9][0-9]\): .*/\1/p' | sort -u)
}

ap_report() { ap_get report; }

# ap_wait_report [seconds]: the last run's report path, once the file exists.
ap_wait_report() {
  local end=$(( $(date +%s) + ${1:-10} )) path
  while :; do
    path="$(ap_report)"
    if [ -n "$path" ] && [ -f "$path" ]; then echo "$path"; return 0; fi
    [ "$(date +%s)" -ge "$end" ] && return 1
    sleep 0.2
  done
}

# ap_wait_notify [seconds]: wait until the configured notify command recorded a call.
ap_wait_notify() {
  local end=$(( $(date +%s) + ${1:-10} ))
  while :; do
    [ -s "$EVALS_WORK/notify.log" ] && return 0
    [ "$(date +%s)" -ge "$end" ] && return 1
    sleep 0.2
  done
}

# ap_wait_log <text> [seconds]: wait until the run's autopilot.log mentions <text>.
ap_wait_log() {
  local text="$1" end=$(( $(date +%s) + ${2:-10} )) dir
  while :; do
    dir="$(ap_get log)"
    if [ -n "$dir" ] && grep -qi -- "$text" "$dir/autopilot.log" 2>/dev/null; then return 0; fi
    [ "$(date +%s)" -ge "$end" ] && return 1
    sleep 0.2
  done
}

# ap_snapshot: everything a user would notice in their checkout: files, the base ref, index.
ap_snapshot() {
  (cd "$PROJ" || return 1
   local base
   base="$(git rev-parse --abbrev-ref HEAD)"
   find . -path ./.git -prune -o -type f -print0 | sort -z | xargs -0 sha256sum
   printf 'BASE %s\n' "$(git rev-parse "refs/heads/$base")"
   git status --porcelain)
}

ap_same_checkout() {
  local before="$1" what="$2" now
  now="$(ap_snapshot)" || return 1
  [ "$before" = "$now" ] && return 0
  fail "$what: $(diff <(printf '%s\n' "$before") <(printf '%s\n' "$now") |
    grep '^[<>]' | head -4 | tr '\n' ' ')"
}

ap_cleanup() {
  local pid
  pid="$(ap_get pid 2>/dev/null)"
  [ -n "$pid" ] && kill -KILL "$pid" 2>/dev/null
  kill_runs 2>/dev/null || true
}
