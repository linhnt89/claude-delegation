# Local-only recording SSH transport for vault checks. Source after common.sh.
# recording_vault <bare repo>: sets VAULT_URL; each connection creates a record.
recording_vault() {
  export EVALS_VAULT_REMOTE="$1"
  export EVALS_VAULT_CONNECTIONS="$EVALS_WORK/vault-connections"
  mkdir -p "$EVALS_VAULT_CONNECTIONS"
  export GIT_SSH_VARIANT=ssh
  export GIT_SSH_COMMAND="\"$EVALS_WORK/bin/evals-vault-ssh\""
  cat >"$EVALS_WORK/bin/evals-vault-ssh" <<'PY'
#!/usr/bin/env python3
import os
import shlex
import sys
import tempfile

# Record even malformed requests; never invoke a real SSH client.
with tempfile.NamedTemporaryFile(dir=os.environ["EVALS_VAULT_CONNECTIONS"], delete=False) as record:
    record.write((" ".join(sys.argv) + "\n").encode())
command = shlex.split(sys.argv[-1])
if (len(command) != 2 or command[0] not in ("git-upload-pack", "git-receive-pack")
        or command[1] != os.environ["EVALS_VAULT_REMOTE"]):
    sys.exit("unexpected local vault transport command")
os.execvp(command[0], command)
PY
  chmod +x "$EVALS_WORK/bin/evals-vault-ssh"
  VAULT_URL="ssh://vault.invalid$EVALS_VAULT_REMOTE"
}

vault_connections() { find "$EVALS_VAULT_CONNECTIONS" -type f | wc -l; }

# Both a pending-change description and an ID or count are required. Do not accept
# a generic offline warning as evidence that a local edit has been accounted for.
hidden_pending_note() {
  python3 -c '
import re, sys
text = sys.stdin.read()
pattern = r"not pushed|unpushed|not on the remote"
pending = re.search(pattern, text, re.I)
identity = re.search(r"\b" + re.escape(sys.argv[1]) + r"\b", text)
count = any(re.search(r"\b[0-9]+\b", line) for line in text.splitlines()
            if re.search(pattern, line, re.I))
hidden_set = re.search(r"hidden|vault", text, re.I)
sys.exit(0 if pending and (identity or (count and hidden_set)) else 1)
' "$1"
}
