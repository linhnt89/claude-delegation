# S078 An HTTPS vault URL without saved credentials says how to fix it
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer connecting a new machine to a vault on their company's internal GitLab, using
the HTTPS clone URL. Git on this machine has no saved credentials for that server. Reported
2026-10-28: setup said only "cannot reach vault URL; check the URL and credentials" followed
by git's "could not read Username … terminal prompts disabled".

## Starting state
A fresh machine. The vault URL is an HTTP(S) remote that asks for a username and password.
No credential helper or saved credentials.

## Steps
1. `delegate setup --vault <url>`.

## Passes when
- It finishes without waiting for input.
- It says git has no saved credentials for this URL, and gives both ways to fix it: use the
  SSH URL instead, or save credentials (a credential helper) by running a git command
  against the URL once in a terminal.
- Nothing is connected: the vault has no remote afterwards.

## If this fails
The user sees "check the URL and credentials" when the URL is fine and git has no way to ask
for credentials, and has to work out for themself that delegate never lets git prompt.

## Failure paths
- A wrong or unreachable URL still gets the existing "check the URL" message (S049).
