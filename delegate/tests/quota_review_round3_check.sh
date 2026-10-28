#!/usr/bin/env bash
# Additional adversarial regressions. Run with quota-status or all-quota-diagnostic.
set -uo pipefail
case_name="${1:?choose quota-status or all-quota-diagnostic}"
export EVALS_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export EVALS_LIB="$EVALS_ROOT/evals/lib"
export EVALS_WORK="$(mktemp -d "$HOME/.cache/t053-round3-${case_name}-XXXXXX")"
echo "Evidence: $EVALS_WORK"
. "$EVALS_LIB/common.sh"
setup_project
workers_toml <<'T'
planner_family = "claude"
[defaults]
implement = "fast"
research = "fast"
review = "standard"
plan_review = "standard"
[implement]
fast = ["claude:claude-first"]
[research]
fast = ["pi:deepseek-research"]
[review]
standard = ["pi:gpt-review", "pi:deepseek-review"]
[plan_review]
standard = ["pi:claude-plan", "pi:deepseek-plan"]
[quota]
check = false
T
install_stub claude
stub_worker claude-first <<'B'
work() {
  printf '\n# partial quota work\n' >>shop/pricing.py
  echo 'Error: usage limit reached'
  return 1
}
B
write_spec t001 'Quota review regression' 'Add cart_total.' \
  "python3 -c 'from shop.pricing import cart_total; assert cart_total([1, 2]) == 3'"
d run t001 --no-ask
printf '%s\n' "$OUT"
[ "$RC" -ne 0 ] || fail 'all-out run should fail'
case "$case_name" in
  all-quota-diagnostic)
    echo "$OUT" | grep -q '^error: no \[implement\]\.fast candidate has quota left:' \
      || fail 'all-out error does not start with the specified quota diagnostic'
    ;;
  quota-status)
    d status t001 --json
    printf '%s\n' "$OUT"
    python3 - "$OUT" <<'PY' || fail 'quota work is reported as no_changes, with discard as the next action'
import json, sys
status = json.loads(sys.argv[1])
status = status['tasks'][0]
assert status['outcome'] != 'no_changes', status
assert 'discard' not in (status.get('next') or ''), status
PY
    ;;
  *) fail "unknown case: $case_name" ;;
esac
pass "$case_name handled correctly"
