# Shared setup for the check-writer family checks (S096, S097, S099).
#
#   scenario_with_check S001 pricing J1 "S001 ok"   # a passing visible scenario
#   write_check path "marker"                       # (re)write a passing check
#   worker_commit t001 pi:deepseek-writer@high "Write S001's check" path...
#                                                  # a merge-like commit with a Worker: trailer
#
# The commits are made on the project's base branch, in the shape `delegate merge` leaves:
# subject "<task id>: <title>", body "Worker: <provider:model@effort>". The checks under test
# read the family from those trailers, so the setup never needs a real worker.

scenario_with_check() { # id slug job [marker]
  local id="$1" slug="$2" job="$3" marker="${4:-$1 ok}"
  local dir="$PROJ/evals/scenarios/$id-$slug"
  mkdir -p "$dir"
  cat >"$dir/scenario.md" <<EOF
# $id $slug
- Job: $job
- Tier: fast
- Judged by: auto
- Status: automated
EOF
  write_check "$dir/check" "$marker"
}

write_check() { # path [marker]
  printf '#!/usr/bin/env bash\necho "%s"\n' "${2:-ok}" >"$1"
  chmod +x "$1"
}

append_check() { # path marker
  printf '\n# %s\n' "$2" >>"$1"
}

worker_commit() { # id worker title path...
  local id="$1" worker="$2" title="$3"
  shift 3
  git -C "$PROJ" add -- "$@"
  git -C "$PROJ" commit -qm "$id: $title" -m "Worker: $worker"
}

commit_sha() { git -C "$PROJ" rev-parse --short "$1"; }
