# Guard eval setup. Use the CLI under test even when an installed hook's command
# says just `delegate ...`, rather than an absolute path to the executable.
. "$EVALS_LIB/common.sh"
unset DELEGATE_GUARD
export PYTHONDONTWRITEBYTECODE=1
