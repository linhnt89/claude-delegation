# S146 Keep real Codex conversations on their own panes without account credentials
- Job: J8
- Tier: full
- Judged by: auto
- Status: agreed
- Timeout: 300

## Who
A developer or regression-check author who needs to reproduce shared-daemon Codex attribution safely without exposing an authenticated profile to workers.

## Starting state
- Explicitly selected real Codex and Herdr binaries, with pinned versions and a documented private daemon provisioning policy. The reproduced baseline is Codex 0.166.0 and Herdr 0.9.5.
- A fresh private HOME/CODEX_HOME/XDG environment, disposable trusted checkout and owned Herdr daemon/socket. No personal credentials/configuration are copied, inherited or accessed.
- A loopback Responses stub returns a minimal assistant reply without tools. Codex uses its custom provider with authentication disabled; no Authorization header is sent and no account login is performed.
- The genuine Herdr Codex SessionStart integration is installed and approved only in the disposable profile, without report-destination or payload overrides.

## Steps
1. Start real interactive Codex A in pane A. Complete a minimal turn against the local provider; capture the actual thread identity and turn completion independently from Codex session evidence.
2. Observe A's source-qualified identity through Herdr's public API, then exit A's TUI while leaving its private managed app-server running. Record A's post-exit state.
3. Start distinct interactive Codex B in pane B using the same CODEX_HOME and daemon; complete another minimal turn against the local provider and independently capture B's real identity.
4. Observe both panes during B startup and after its completed reply. Verify that both threads were served by the same daemon process instance, not merely that an unused daemon stayed alive.
5. Clean up all owned test processes, including any private updater and local provider, without touching personal daemons or profiles.

## Passes when
- Both real turns complete and their independently observed thread identities differ.
- While each TUI is running, its pane reports its own real Codex identity after a bounded startup reporting interval, including the post-reply observation window.
- During B's launch and execution, pane A never observably publishes B's identity. A may retain A or become explicitly unknown after A exits.
- Public-API observation covers required phases with a documented cadence and maximum gap; missing phases or read failures cannot silently pass.
- The same managed daemon serves both threads without restart or per-launch bypass. Only model replies are stubbed, never conversation identities, hook execution or identity publication.
- No account credentials are needed or transmitted; cleanup verifies no owned test processes remain.

## If this fails
A second real Codex conversation can be assigned to another pane even without account authentication, allowing a task dashboard to group work under the wrong lead.

## Failure paths
- Absent explicit opt-in or prerequisites is an honest skip, not an attribution pass. Opted-in broken setup, insufficient evidence/coverage, daemon replacement or cleanup failure is a check error.
- Once valid setup is established, wrong-pane publication or persistent missing/mismatched identity at the reporting deadline is product failure. Later observation errors must not erase a concrete wrong-pane counterexample.
- Timeouts and interruption must trigger bounded owned cleanup before the outer scenario timeout; a partial run cannot pass.
- Raw identities, terminal output and session/request evidence stay in private temporary files, never in committed reports.
- This credential-free sibling does not replace authenticated S145, simultaneous/resume tests, mixed-version coverage or native observer acceptance.
