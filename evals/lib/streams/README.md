# Recorded worker streams

What each real harness prints in its streaming JSON mode, so the stand-in workers in checks
can replay it (`replay <harness>` in a behaviour file, see `common.sh`).

| File | What |
|---|---|
| `<harness>.jsonl` | The raw stream, recorded from the real harness running the script below |
| `<harness>.steps` | The steps `delegate status <id> --json` must show for it, in order: one per line, `line<TAB>kind<TAB>target<TAB>result`. `line` is the stream line that first announces the step; `target` must appear in the shown target; `result` (may be empty) must appear in the shown result |
| `<harness>.version` | The harness version, model and streaming flags it was recorded with |

Harnesses with step adapters: `claude`, `codex`, `opencode`, `pi`. Re-record when a harness
changes its format; a missing recording fails S023 and P010 for that harness. Gemini has no
adapter for now (2026-10-25): its runs get the basic view, like any worker without one.

Step rules the recordings follow: one step per tool call (two edits of one file are two
steps); a command's result starts with `ok` when it exits 0 and `failed` otherwise; a reply
to the user is a `message`, with its first words as the target; empty (redacted) thinking
is not a step.

`<harness>-fail.*` are a second recording per harness in which a command fails before it
passes (prompt below), so a failed result is checked too.

## The recording script
In the `setup_project` shop repo (`evals/lib/common.sh`), run the harness the way `delegate`
runs a work phase, in its streaming mode, with this prompt:

> Read shop/pricing.py and tests/test_pricing.py. Add `divide_bill(amount, people)` to
> shop/pricing.py that returns the amount split evenly, rounded to cents. Run
> `python3 -m unittest -q`. Then add a test for it to tests/test_pricing.py and run the tests
> again. Reply with one line saying what you did.

Before committing a recording, scrub it: the work folder becomes `/work/shop`, the user's
home `/home/user`; drop system prompts, tool descriptions, plugin, skill and MCP lists (keep
tool names). Streams split across many events (e.g. Pi's `toolcall_delta`) must be checked
joined up, since a path can be cut in pieces.

The `-fail` recordings use this prompt instead:

> First, run exactly `python3 -m unittest -q tests.test_split` on its own (no other commands
> chained, no `echo`). It fails because tests/test_split.py does not exist yet. Then create
> tests/test_split.py with one test that checks `subtotal([(1.0, 2)]) == 2.0` (from
> shop.pricing), and run exactly the same command again. Reply with one line saying what you did.

`<harness>.steps` is written by the lead from reading the stream, not by whoever writes the
adapter, so the adapter can't be tailored to it.
