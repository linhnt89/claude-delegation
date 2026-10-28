#!/usr/bin/env bash
# Adversarial regressions for t053. Run: bash delegate/tests/quota_review_check.sh <case>
set -uo pipefail
case_name="${1:?choose stderr, unterminated, allowed-event, stale-report, early-quota, or research}"
export EVALS_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export EVALS_LIB="$EVALS_ROOT/evals/lib"
# /tmp is intentionally hidden from sandboxed workers, so keep fixtures outside it.
export EVALS_WORK="$(mktemp -d "$HOME/.cache/t053-review-${case_name}-XXXXXX")"
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
fast = ["claude:claude-first", "codex:codex-second", "pi:gpt-third"]
standard = ["pi:gpt-third"]
deep = ["pi:gpt-third"]
[research]
fast = ["claude:claude-first", "codex:codex-second"]
[review]
standard = ["pi:gpt-review", "pi:deepseek-review"]
[plan_review]
standard = ["pi:claude-plan", "pi:deepseek-plan"]
[quota]
check = false
T
install_stub claude
install_stub codex
stub_worker codex-second <<'B'
work() { printf '\n\ndef cart_total(items):\n    return sum(items)\n' >>shop/pricing.py; }
research() { printf '# Finished research\n' >.task/REPORT.md; }
B
stub_worker gpt-third <<'B'
work() { printf '\n\ndef cart_total(items):\n    return sum(items)\n' >>shop/pricing.py; }
B
case "$case_name" in
  stderr)
    stub_worker claude-first <<'B'
work() { printf '\n# partial work\n' >>shop/pricing.py; echo 'Error: usage limit reached' >&2; return 1; }
B
    ;;
  unterminated)
    stub_worker claude-first <<'B'
work() { printf '\n# partial work\n' >>shop/pricing.py; printf 'Error: usage limit reached'; return 1; }
B
    ;;
  allowed-event)
    stub_worker claude-first <<'B'
work() {
  printf '\n# partial work\n' >>shop/pricing.py
  printf '%s\n' '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1792963200,"rateLimitType":"five_hour"}}'
  echo 'Error: unrelated internal failure'
  return 1
}
B
    ;;
  stale-report)
    stub_worker claude-first <<'B'
work() {
  printf '\n# partial work\n' >>shop/pricing.py
  printf '# First worker partial report\n' >.task/REPORT.md
  echo 'Error: usage limit reached'
  return 1
}
B
    stub_worker codex-second <<'B'
start() { echo 'Error: authentication required'; exit 1; }
B
    ;;
  early-quota)
    workers_toml <<'T'
planner_family = "claude"
[defaults]
implement = "fast"
research = "fast"
review = "standard"
plan_review = "standard"
[implement]
fast = ["claude:claude-first", "claude:claude-same-account", "codex:codex-second"]
[research]
fast = ["pi:deepseek-research"]
[review]
standard = ["pi:gpt-review", "pi:deepseek-review"]
[plan_review]
standard = ["pi:claude-plan", "pi:deepseek-plan"]
[quota]
check = false
T
    stub_worker claude-first <<'B'
work() { echo 'Error: usage limit reached'; return 1; }
B
    stub_worker claude-same-account <<'B'
work() { echo 'same account must have been skipped'; return 1; }
B
    ;;
  research)
    stub_worker claude-first <<'B'
research() { printf '# Partial research\n' >.task/REPORT.md; echo 'Error: usage limit reached'; return 1; }
B
    ;;
  *) fail "unknown case: $case_name" ;;
esac
write_spec t001 'Quota review regression' 'Add cart_total.' \
  "python3 -c 'from shop.pricing import cart_total; assert cart_total([1, 2]) == 3'"
if [ "$case_name" = research ]; then
  d run t001 --research
else
  d run t001 --no-ask
fi
printf '%s\n' "$OUT"
case "$case_name" in
  allowed-event)
    [ -z "$(calls_where work codex-second)" ] || fail 'an allowed rate-limit status was mistaken for quota exhaustion'
    ;;
  stale-report)
    [ -n "$(calls_where work gpt-third)" ] || fail 'stale first-worker REPORT.md prevents fallback after the second candidate fails to start'
    ;;
  early-quota)
    [ -z "$(calls_where work claude-same-account)" ] || fail 'the next model on the already-exhausted account was started'
    ;;
  research)
    [ -n "$(calls_where research codex-second)" ] || fail 'a quota-limited research round did not hand off'
    ;;
  *)
    [ -n "$(calls_where work codex-second)" ] || fail "$case_name quota output did not hand off"
    ;;
esac
pass "$case_name handled correctly"
