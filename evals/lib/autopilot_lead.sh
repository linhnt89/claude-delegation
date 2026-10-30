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
