# Shared setup for checks: source it from a check script.
#
#   . "$EVALS_LIB/common.sh"
#   setup_project            # $PROJ: a small git repo with code and tests, kit files added
#   stub_worker gpt-good <<'EOF' ... EOF   # how the stand-in model named gpt-good behaves
#   workers_toml <<'EOF' ... EOF           # the user's worker config
#   write_spec t001 "Add divide" ...       # a ready spec
#   d run t001                             # run delegate in $PROJ; output in $OUT, code in $RC
#
# Stand-in workers: `pi` on PATH is a script (`install_stub <harness>` adds the others). The
# model name in a candidate (`pi:gpt-good@high`) picks the behaviour file
# $EVALS_WORK/stub/<model>; the family is still inferred from the name (gpt, deepseek,
# claude...), so the real routing rules apply.
# Every call is recorded under $HOME/.cache/stub/calls/<n>/ (model, phase, prompt, spec).
# A behaviour can `replay <harness>` to print that harness's recorded event stream
# ($EVALS_LIB/streams/), like the real worker would.

set -uo pipefail

fail() { echo "FAIL: $(printf "%s" "$*" | tr "\n" " " | cut -c1-400)"; exit 1; }
pass() { echo "$*"; exit 0; }
note() { echo "-- $*"; }

# The binary under test, built from this checkout (a worktree builds its own). Built with the
# real HOME, before checks point HOME at a scratch folder.
delegate_bin() {
  local manifest="$EVALS_ROOT/delegate/Cargo.toml"
  if ! (cd "$EVALS_ROOT/delegate" && export MISE_STATE_DIR="${MISE_STATE_DIR:-$HOME/.cache/mise-state}" \
          MISE_TRUSTED_CONFIG_PATHS="$PWD" && mise exec -- cargo build -q --release --offline) >"$EVALS_WORK/build.log" 2>&1; then
    cat "$EVALS_WORK/build.log"
    fail "could not build delegate from $manifest (see build.log)"
  fi
  echo "$EVALS_ROOT/delegate/target/release/delegate"
}

export DELEGATE="${DELEGATE:-$(delegate_bin)}"
export REAL_HOME="$HOME"
export HOME="$EVALS_WORK/home"
mkdir -p "$HOME/.cache/stub/calls" "$HOME/.pi" "$HOME/.config/delegate"
mkdir -p "$EVALS_WORK/bin" "$EVALS_WORK/stub"
# `delegate` is on the user's PATH, so the project's evals/run and skills can call it.
ln -sf "$DELEGATE" "$EVALS_WORK/bin/delegate"
export PATH="$EVALS_WORK/bin:$PATH"
# The lead running the evals is itself a Claude Code session: its id must not own test tasks.
unset DELEGATE_HOME DELEGATE_WORKERS DELEGATE_SANDBOX XDG_DATA_HOME XDG_CONFIG_HOME CLAUDE_CODE_SESSION_ID
# Nothing a check starts (delegate, workers, a Claude under test) may find the hidden set
# or write into this run's results.
unset EVALS_HIDDEN EVALS_RESULTS
export GIT_AUTHOR_NAME=Eval GIT_AUTHOR_EMAIL=eval@example.test
export GIT_COMMITTER_NAME=Eval GIT_COMMITTER_EMAIL=eval@example.test
export GIT_CONFIG_GLOBAL="$HOME/.gitconfig"
git config --global init.defaultBranch main
git config --global advice.detachedHead false

export PROJ="$EVALS_WORK/shop"
export DHOME="$EVALS_WORK/shop.delegate"

# dhome_of <checkout>: its task-state folder, as `delegate where` says; before `where` existed,
# <checkout>.delegate beside it. hidden_of <checkout>: its hidden scenarios folder, if any.
dhome_of() {
  local w
  w="$(cd "$1" 2>/dev/null && "$DELEGATE" where 2>/dev/null | awk '$1 == "tasks" {sub(/^tasks[ \t]+/, ""); print; exit}')"
  [ -n "$w" ] && echo "$w" || echo "${1%/}.delegate"
}
hidden_of() {
  (cd "$1" 2>/dev/null && "$DELEGATE" where 2>/dev/null | awk '$1 == "hidden" {sub(/^hidden[ \t]+/, ""); print; exit}')
}

# d <args...>: run delegate in $PROJ. Sets OUT (stdout+stderr) and RC.
d() {
  OUT="$(cd "${DCWD:-$PROJ}" && "$DELEGATE" "$@" 2>&1)"; RC=$?
  printf '\n$ delegate %s  (exit %s)\n%s\n' "$*" "$RC" "$OUT" >>"$EVALS_WORK/transcript.log"
  [ -d "$PROJ" ] && DHOME="$(dhome_of "$PROJ")"
  return 0
}

# A small but real project: a price calculator with tests, two commits.
setup_project() {
  mkdir -p "$PROJ/shop" "$PROJ/tests"
  cd "$PROJ" && git init -q
  cat >shop/__init__.py <<'EOF'
EOF
  cat >shop/pricing.py <<'EOF'
"""Prices for the shop's checkout."""


def subtotal(items):
    """Sum of price * quantity over (price, quantity) pairs."""
    return sum(price * qty for price, qty in items)


def with_tax(amount, rate):
    """Amount plus tax at `rate` (0.2 means 20%), rounded to cents."""
    return round(amount * (1 + rate), 2)
EOF
  cat >tests/test_pricing.py <<'EOF'
import unittest

from shop.pricing import subtotal, with_tax


class PricingTest(unittest.TestCase):
    def test_subtotal(self):
        self.assertEqual(subtotal([(2.5, 2), (1.0, 3)]), 8.0)

    def test_with_tax(self):
        self.assertEqual(with_tax(10.0, 0.2), 12.0)


if __name__ == "__main__":
    unittest.main()
EOF
  : >tests/__init__.py
  printf '# Shop\n\nCheckout pricing. Test: `python3 -m unittest -q`\n' >README.md
  git add -A && git commit -qm "Pricing: subtotal and tax"
  printf '\n\ndef discount(amount, percent):\n    """Amount after a percentage discount."""\n    return round(amount * (1 - percent / 100), 2)\n' >>shop/pricing.py
  git commit -qam "Add discount"
  if [ "${1:-}" != "--no-init" ]; then
    "$DELEGATE" init >/dev/null 2>&1 || fail "delegate init failed in setup"
    git add -A && git commit -qm "Add cooperation kit"
  fi
  cd "$EVALS_WORK"
}

# workers_toml: the user's worker config from stdin.
workers_toml() { cat >"$HOME/.config/delegate/workers.toml"; }

# A config where every role uses stand-in models named after their behaviour.
simple_workers() {
  workers_toml <<EOF
planner_family = "claude"
[defaults]
implement = "${1:-standard}"
research = "fast"
review = "standard"
plan_review = "standard"
[implement]
fast = ["pi:${2:-gpt-good}@high"]
standard = ["pi:${3:-gpt-good}@high"]
deep = ["pi:${4:-gpt-good}@high"]
[research]
fast = ["pi:deepseek-research@medium"]
[review]
standard = ["pi:gpt-review@high", "pi:deepseek-review@high"]
[plan_review]
standard = ["pi:claude-plan@high", "pi:deepseek-plan@high"]
EOF
}

# write_spec <id> <title> <goal> <acceptance command>...: a ready spec for the task.
write_spec() {
  local id="$1" title="$2" goal="$3"; shift 3
  d new "$id"
  [ "$RC" -eq 0 ] || fail "delegate new $id failed: $OUT"
  local spec="${OUT##*$'\n'}"
  mkdir -p "$(dirname "$spec")"
  {
    echo "# Task $id: $title"
    echo
    echo "## Goal"
    echo "$goal"
    echo
    echo "## Context (read these, nothing more)"
    echo '- `AGENTS.md`'
    echo '- `shop/pricing.py`'
    echo
    echo "## Scope"
    echo "- In: ${SCOPE:-\`shop/pricing.py\`, \`tests/**\`}"
    echo "- Out: everything else"
    echo
    echo "## Acceptance"
    for c in "$@"; do echo "- [ ] \`$c\`"; done
    echo
    echo "## If unsure"
    echo "STOP. Don't guess. Write the question in .task/REPORT.md and finish."
  } >"$spec"
}

# stub_worker <model>: behaviour for one stand-in model, as bash functions read from stdin.
# Functions it may define (each runs in the worktree, or the main checkout for plan review):
#   ask      question phase; default writes READY to .task/QUESTIONS.md
#   work     work phase; default does nothing
#   review   review; default writes "FINDINGS: 0 high, 0 medium, 0 low"
#   plan     plan review; default prints a findings line
#   research research; default writes a report
#   start    runs first in every call; `exit 1` there means "could not start"
stub_worker() { cat >"$EVALS_WORK/stub/$1"; }

# Calls recorded by the stand-ins: one folder per call, in order.
calls() { ls -1 "$HOME/.cache/stub/calls" 2>/dev/null | sort -n; }
# calls_where <phase> [model]: call ids of that phase (and model), in order.
calls_where() {
  local c
  for c in $(calls); do
    [ "$(call_field "$c" phase)" = "$1" ] || continue
    [ -z "${2:-}" ] || [ "$(call_field "$c" model)" = "$2" ] || continue
    echo "$c"
  done
}
flag() { mkdir -p "$EVALS_WORK/flags"; printf '%s\n' "${2:-1}" >"$EVALS_WORK/flags/$1"; }
unflag() { rm -f "$EVALS_WORK/flags/$1"; }
call_field() { cat "$HOME/.cache/stub/calls/$1/$2" 2>/dev/null; }

# install_stub <harness>: put a stand-in for that harness on PATH (pi is always there).
install_stub() {
  cat >"$EVALS_WORK/bin/$1" <<'STUB'
#!/usr/bin/env bash
# Stand-in for a harness, driven by $EVALS_WORK/stub/<model>. The prompt is the last argument.
model="" prompt="${@: -1}"
while [ $# -gt 1 ]; do
  case "$1" in --model|-m) model="$2"; shift ;; esac
  shift
done
case "$prompt" in
  *"Do NOT edit any files yet"*) phase=ask ;;
  *"adversarial reviewer"*) phase=review ;;
  *"reviewing a plan"*) phase=plan ;;
  *"delegated researcher"*) phase=research ;;
  *) phase=work ;;
esac
calls="$HOME/.cache/stub/calls"
n="$(date +%s%N)-$$"
mkdir -p "$calls/$n"
printf '%s\n' "$model" >"$calls/$n/model"
printf '%s\n' "$phase" >"$calls/$n/phase"
printf '%s\n' "$prompt" >"$calls/$n/prompt"
pwd >"$calls/$n/cwd"
[ -f .task/SPEC.md ] && cp .task/SPEC.md "$calls/$n/spec"
# Everything the worker can see in .task/, so checks can ask "was it told X?"
[ -d .task ] && cp -r .task "$calls/$n/task"

ask() { printf 'READY\n' >.task/QUESTIONS.md; }
work() { :; }
review() { printf 'FINDINGS: 0 high, 0 medium, 0 low\nNothing found.\n' >.task/REVIEW.md; }
plan() { printf 'FINDINGS: 1 high, 0 medium, 0 low\nhigh, t001, acceptance checks do not prove the goal, add a test\n'; }
research() { printf '# Report\n\nsubtotal is in shop/pricing.py:4\n' >.task/REPORT.md; }
start() { :; }
wait_flag() {
  local i
  for i in $(seq $(( ${2:-120} * 5 ))); do
    [ -f "$EVALS_WORK/flags/$1" ] && { cat "$EVALS_WORK/flags/$1"; return 0; }
    sleep 0.2
  done
  return 1
}
# replay <harness|file> [delay]: print a recorded stream a line at a time, noting when each
# line went out in $HOME/.cache/stub/replayed (line number, epoch seconds).
replay() {
  local f="$1" n=0 line
  [ -f "$f" ] || f="$EVALS_LIB/streams/$1.jsonl"
  [ -f "$f" ] || { echo "no recorded stream $f" >&2; return 1; }
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1)); printf '%s\n' "$line"
    printf '%s %s\n' "$n" "$(date +%s.%N)" >>"$HOME/.cache/stub/replayed"
    sleep "${2:-0.5}"
  done <"$f"
}
behaviour="$EVALS_WORK/stub/$model"
[ -f "$behaviour" ] && . "$behaviour"
start
"$phase"
STUB
  chmod +x "$EVALS_WORK/bin/$1"
}
install_stub pi

# bg_run <id>: `delegate run --background <id>` from a shell that then exits. Sets OUT and RC.
bg_run() {
  OUT="$(cd "$PROJ" && timeout 20 "$DELEGATE" run --background "$@" 2>&1 </dev/null)"; RC=$?
  printf '\n$ delegate run --background %s  (exit %s)\n%s\n' "$*" "$RC" "$OUT" >>"$EVALS_WORK/transcript.log"
  DHOME="$(dhome_of "$PROJ")"
}

# sj <expr> [id]: a value from `delegate status [id] --json`, by a Python expression over `s`
# (the parsed output), e.g. sj 's["tasks"][0]["phase"]' t001. Empty when status fails.
sj() {
  (cd "$PROJ" 2>/dev/null && "$DELEGATE" status ${2:+"$2"} --json 2>/dev/null) |
    python3 -c 'import json,sys
try: s=json.load(sys.stdin)
except Exception: sys.exit(0)
try: v=eval(sys.argv[1])
except Exception: sys.exit(0)
print(json.dumps(v) if isinstance(v,(dict,list,bool)) or v is None else v)' "$1"
}

# wait_flag <name> [seconds]: wait (default 120s) until a check sets that flag; prints its value.
# Defined for behaviour files too: they run inside the stand-in.
wait_flag() {
  local i
  for i in $(seq $(( ${2:-120} * 5 ))); do
    [ -f "$EVALS_WORK/flags/$1" ] && { cat "$EVALS_WORK/flags/$1"; return 0; }
    sleep 0.2
  done
  return 1
}

# wait_until <seconds> <command...>: poll every 0.2s until the command succeeds. Arguments are
# expanded once, before polling: to poll status, use wait_field or wait_changed.
wait_until() {
  local end=$(( $(date +%s) + $1 )); shift
  until "$@"; do [ "$(date +%s)" -ge "$end" ] && return 1; sleep 0.2; done
}

# wait_field <seconds> <id> <field> <value>: poll status --json until the task's field has that value.
wait_field() {
  local end=$(( $(date +%s) + $1 ))
  until [ "$(sj "s['tasks'][0]['$3']" "$2")" = "$4" ]; do [ "$(date +%s)" -ge "$end" ] && return 1; sleep 0.2; done
}
# wait_changed <seconds> <id> <text>: poll until the task's changed_files mention the text.
wait_changed() {
  local end=$(( $(date +%s) + $1 ))
  until sj "s['tasks'][0]['changed_files']" "$2" | grep -qF "$3"; do [ "$(date +%s)" -ge "$end" ] && return 1; sleep 0.2; done
}

# kill_runs: stop this check's background runs and stand-in workers (`trap kill_runs EXIT`).
kill_runs() {
  local p
  for p in $(sj '" ".join(str(t["pid"]) for t in s["tasks"] if t.get("pid"))'); do
    kill -KILL -- -"$p" 2>/dev/null; kill -KILL "$p" 2>/dev/null
  done
  pkill -KILL -f "$EVALS_WORK/bin/" 2>/dev/null
  true
}

# minimal_path <tool>...: PATH with only the stand-ins and the named tools, so "missing" is real.
minimal_path() {
  local dir="$EVALS_WORK/minbin" t
  # Tools come from the PATH before the first call, so calling it again (PATH is minbin by then)
  # still finds them.
  : "${FULL_PATH:=$PATH}"
  PATH="$FULL_PATH" rm -rf "$dir" && PATH="$FULL_PATH" mkdir -p "$dir"   # fresh each call: a tool linked earlier must not linger
  for t in "$@" git bash sh env cat ls mkdir cp rm wc date grep sed sort tail head printf python3 dirname basename; do
    [ -e "$dir/$t" ] || PATH="$FULL_PATH" ln -s "$(PATH="$FULL_PATH" command -v "$t")" "$dir/$t" 2>/dev/null
  done
  export PATH="$EVALS_WORK/bin:$dir"
}

# same_main <before> <what>: fail with the first difference if the main checkout changed.
same_main() {
  local now; now="$(snapshot_main)"
  [ "$1" = "$now" ] && return 0
  fail "$2: $(diff <(echo "$1") <(echo "$now") | grep '^[<>]' | head -2 | tr '\n' ' ')"
}

# Snapshot of everything a user would care about in the main checkout: files, refs, index.
snapshot_main() {
  (cd "$PROJ" && {
    find . -path ./.git -prune -o -type f -print0 | sort -z | xargs -0 sha256sum
    git for-each-ref --format='%(refname) %(objectname)' | grep -v '^refs/heads/task/' || true
    git rev-parse HEAD
    git status --porcelain
  })
}

# old_kit_project <commit>: $PROJ set up by the kit at that commit (see kits.py), committed.
# Like a project set up before `delegate upgrade` existed: no record of its kit version.
old_kit_project() {
  setup_project --no-init
  cd "$PROJ" && python3 "$EVALS_LIB/kits.py" install "$1" "$PROJ" || fail "could not rebuild the kit at $1"
  git add -A && git commit -qm "Add cooperation kit" && cd "$EVALS_WORK"
}

# fresh_kit <dir>: what the current `delegate init` installs, in a new repo at <dir>.
fresh_kit() {
  mkdir -p "$1" && git -C "$1" init -q && (cd "$1" && "$DELEGATE" init >/dev/null 2>&1) ||
    fail "delegate init failed in a new repo"
}

# newer_delegate <name>: a delegate binary from a copy of this checkout whose kit was changed
# by the bash commands on stdin (run in the copy's delegate/templates/project/). Prints its
# path. Builds share one target dir, so only the crate itself recompiles.
newer_delegate() {
  local src="$EVALS_WORK/src-$1" out="$EVALS_WORK/bin-$1/delegate"
  local target="$EVALS_ROOT/delegate/target/evals-newer"
  mkdir -p "$src" "$(dirname "$out")"
  tar -C "$EVALS_ROOT" --exclude=delegate/target -cf - delegate | tar -C "$src" -xf -
  (cd "$src/delegate/templates/project" && bash -e) || fail "could not change the kit for $1"
  if ! (cd "$src/delegate" && export MISE_STATE_DIR="${MISE_STATE_DIR:-$REAL_HOME/.cache/mise-state}" \
          MISE_TRUSTED_CONFIG_PATHS="$PWD" HOME="$REAL_HOME" CARGO_TARGET_DIR="$target" &&
          flock "$EVALS_ROOT/delegate/target" mise exec -- cargo build -q --release --offline &&
          cp "$target/release/delegate" "$out") >"$EVALS_WORK/build-$1.log" 2>&1; then
    fail "could not build the newer delegate $1 (see build-$1.log)"
  fi
  echo "$out"
}

# kit_files <dir>: "<mode> <sha256> <path>" for every file under <dir> but .git, sorted.
kit_files() {
  (cd "$1" && find . -path ./.git -prune -o -type f -printf '%m %p\n' | sort -k2 |
    while read -r m p; do echo "$m $(sha256sum "$p" | cut -c1-64) ${p#./}"; done)
}
# same_kit_file <mode> <sha256> <file>: the file exists with that mode and content.
same_kit_file() {
  [ -f "$3" ] && [ "$(stat -c %a "$3")" = "$1" ] && [ "$(sha256sum "$3" | cut -c1-64)" = "$2" ]
}

# --- Machines, vaults and hidden scenarios (decisions 007, 008) ---

# use_machine <name>: act as another machine from now on: its own HOME (config, vault, task
# state), same PATH and binary. The first machine is the HOME common.sh set up ("main").
use_machine() {
  if [ "$1" = main ]; then export HOME="$EVALS_WORK/home"; else export HOME="$EVALS_WORK/machines/$1/home"; fi
  mkdir -p "$HOME/.cache/stub/calls" "$HOME/.pi" "$HOME/.config/delegate"
  export GIT_CONFIG_GLOBAL="$HOME/.gitconfig"
  git config --global init.defaultBranch main
  git config --global advice.detachedHead false
}

# bare_remote <name>: an empty bare git repo standing in for a private remote; prints its URL.
bare_remote() {
  local r="$EVALS_WORK/remotes/$1.git"
  [ -d "$r" ] || git init -q --bare "$r"
  echo "$r"
}

# setup_machine <vault url>: `delegate setup --vault <url>` on the current machine, or fail.
setup_machine() {
  OUT="$("$DELEGATE" setup --vault "$1" </dev/null 2>&1)"; RC=$?
  printf '\n$ delegate setup --vault %s  (exit %s)\n%s\n' "$1" "$RC" "$OUT" >>"$EVALS_WORK/transcript.log"
  [ "$RC" -eq 0 ] || fail "setup: delegate setup --vault failed: $OUT"
}

# hide_scenario <id> [marker]: write a passing scenario in $PROJ/evals/scenarios and hide it with
# `delegate hidden add`, as /evaluate does. Its title is "Hidden check <marker>", which
# evals/run prints as "Hidden check <marker>  (<id>, hidden)".
hide_scenario() {
  local dir="$PROJ/evals/scenarios/$1-hidden-check"
  mkdir -p "$dir"
  printf '# %s Hidden check %s\n- Job: J1\n- Tier: fast\n- Judged by: auto\n- Status: automated\n' "$1" "${2:-v1}" >"$dir/scenario.md"
  printf '#!/usr/bin/env bash\necho "hidden %s %s"\n' "$1" "${2:-v1}" >"$dir/check"
  chmod +x "$dir/check"
  d hidden add "evals/scenarios/$1-hidden-check"
  [ "$RC" -eq 0 ] || fail "setup: delegate hidden add $1 failed: $OUT"
}

# remote_has <url> <text>: some file in the remote's main branch contains the text.
remote_has() { git --git-dir="$1" grep -q -F "$2" main -- 2>/dev/null; }

# project_evals <checkout>: run the project's own evals/run there; sets EV (output) and ERC.
project_evals() {
  EV="$(cd "$1" && evals/run fast 2>&1)"; ERC=$?
  printf '\n$ (%s) evals/run fast  (exit %s)\n%s\n' "$1" "$ERC" "$EV" >>"$EVALS_WORK/transcript.log"
}

# delegate_at <commit>: a delegate binary built from that commit of this repo (an older one,
# for starting states made by earlier versions). Prints its path.
delegate_at() {
  local src="$EVALS_WORK/src-$1" out="$EVALS_WORK/bin-$1/delegate"
  local target="$EVALS_ROOT/delegate/target/evals-newer"
  mkdir -p "$src" "$(dirname "$out")"
  git -C "$EVALS_ROOT" archive "$1" delegate | tar -C "$src" -xf - || fail "no commit $1 in this repo"
  if ! (cd "$src/delegate" && export MISE_STATE_DIR="${MISE_STATE_DIR:-$REAL_HOME/.cache/mise-state}" \
          MISE_TRUSTED_CONFIG_PATHS="$PWD" HOME="$REAL_HOME" CARGO_TARGET_DIR="$target" &&
          flock "$EVALS_ROOT/delegate/target" mise exec -- cargo build -q --release --offline &&
          cp "$target/release/delegate" "$out") >"$EVALS_WORK/build-$1.log" 2>&1; then
    fail "could not build delegate at $1 (see build-$1.log)"
  fi
  echo "$out"
}
