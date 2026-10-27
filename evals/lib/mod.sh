# Checks for the delegate pane mod, run through Claude Code's own mod test runner
# (`claude plugin test`). Source after common.sh.
#
#   mod_setup                     # $MOD: a copy of the mod `delegate init` installed in $PROJ
#   mod_check "$EVALS_SCENARIO/mod.test.tsx"   # run that scenario's tests against it
#
# The scenario's tests drive the mod the way Claude Code does (session start, clock ticks, the
# pane, the row above the prompt) with a stand-in host (mod-host.tsx) answering its calls,
# `delegate status --json` among them. The mod's own unit tests are left out.

mod_setup() {
  command -v claude >/dev/null || fail "the pane checks need Claude Code (\`claude\`) on PATH"
  [ -d "$PROJ/.claude/skills/delegate-pane" ] || fail "delegate init didn't install the pane mod in .claude/skills/delegate-pane"
  MOD="$EVALS_WORK/mod"
  rm -rf "$MOD" && cp -r "$PROJ/.claude/skills/delegate-pane" "$MOD" || fail "setup: could not copy the pane mod"
  find "$MOD" -name '*.test.ts' -delete -o -name '*.test.tsx' -delete
  cp "$EVALS_LIB/mod-host.tsx" "$MOD/hooks/mod-host.tsx"
}

# mod_check <test file>: copy it into the mod as hooks/scenario.test.tsx and run it.
# Fails with the names of the failing tests, which say what the user would see go wrong.
mod_check() {
  [ -n "${MOD:-}" ] || mod_setup
  cp "$1" "$MOD/hooks/scenario.test.tsx"
  OUT="$(cd "$MOD" && timeout 120 claude plugin test "$MOD" 2>&1)"; RC=$?
  printf '\n$ claude plugin test (exit %s)\n%s\n' "$RC" "$OUT" >>"$EVALS_WORK/transcript.log"
  [ "$RC" -eq 0 ] && return 0
  printf '%s\n' "$OUT" | grep -E '^\((pass|fail)\)|^  (Expected|Received)' | sed 's/^/-- /'
  local failed why
  failed="$(printf '%s\n' "$OUT" | sed -n 's/^(fail) \(.*\) \[[0-9.]*m\?s\]$/\1/p' | head -3 | paste -sd ';' -)"
  why="$(printf '%s\n' "$OUT" | grep -m1 -E 'Error|Expected|Received' | sed 's/^ *//')"
  fail "${failed:-the pane's checks didn't run} (${why:-see transcript.log})"
}
