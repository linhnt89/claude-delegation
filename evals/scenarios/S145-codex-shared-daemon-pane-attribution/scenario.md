# S145 Keep Codex conversations distinct when two panes share its background daemon
- Job: J8
- Tier: full
- Judged by: auto
- Status: agreed
- Timeout: 300

## Who
A developer running Codex leads in Herdr panes, using one authenticated Codex profile and its normal shared background app-server daemon.

## Starting state
- An explicitly opted-in, isolated Herdr daemon, disposable checkout and authenticated temporary Codex profile. No personal credentials/configuration are copied and no personal daemon is stopped.
- Installed real Codex and Herdr binaries are pinned and their versions recorded. The reproduced baseline used Codex 0.166.0 and Herdr 0.9.5.
- The genuine Herdr Codex SessionStart integration is installed and approved in that temporary profile; the disposable checkout is trusted.
- Only the private Codex daemon may be stopped once before the sequence to establish a known baseline. Both conversations then use the same CODEX_HOME and the same daemon instance.

## Steps
1. Start real interactive Codex conversation A in pane A and submit a minimal no-tools prompt. Capture its actual conversation identity independently from Codex's own session evidence, and observe the identity exposed for pane A through Herdr's public API.
2. Exit A's TUI while leaving the private Codex app-server daemon running. Record pane A's post-exit identity state.
3. Start a distinct real interactive Codex conversation B in a different pane B, with the same temporary profile and still-running daemon. Submit the same minimal prompt and independently capture B's actual identity.
4. Observe both panes through Herdr's public API during B's startup and after its reply. Verify that the daemon instance did not change between conversations.
5. Stop only the private Codex daemon and the owned private Herdr child; leave normal profiles, daemons and delegate task execution untouched.

## Passes when
- Both real conversations complete their minimal prompts, and their independently observed conversation identities differ.
- While A is running, Herdr identifies pane A as A; while B is running, Herdr identifies pane B as B, using the real Codex integration rather than a fabricated or manually reported identity.
- Starting B never publishes B's identity on pane A. Pane A may retain A or become explicitly unknown after A exits, according to Herdr's lifecycle rules; it must not become B.
- Correct attribution does not require restarting the shared daemon between conversations, copying credentials, giving every pane a different Codex profile, or guessing conversation identity from pane/workspace/model names.
- Any timeout, missing identity or unsupported integration is reported honestly; unknown is safer than wrong ownership but is not a passing result for this scenario.

## If this fails
A second Codex conversation can overwrite another pane's identity, so the task dashboard may put work under the wrong lead even though both conversations run successfully.

## Failure paths
- Missing explicit opt-in, authentication, required binaries or independent Codex identity evidence is unavailable setup, not a passing attribution result. The future check must distinguish this from a product failure.
- Once valid setup is established, a missing identity, wrong-pane report or identity mismatch is a product failure.
- If the daemon exits or is replaced between conversations, the check cannot claim shared-daemon coverage; report a check error rather than a pass.
- Diagnostic hook tracing may record real private evidence, but must not change the hook's report destination or payload to make attribution pass. Do not commit credentials, raw conversation ids or terminal transcripts.
- This scenario is a focused integration regression. It does not by itself pass the native observer scenarios, production ownership properties, or simultaneous multi-lead acceptance.
