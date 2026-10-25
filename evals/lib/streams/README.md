# Recorded worker streams

What each real harness prints in its streaming JSON mode, so the stand-in workers in checks
can replay it (`replay <harness>` in a behaviour file, see `common.sh`).

| File | What |
|---|---|
| `<harness>.jsonl` | The raw stream, recorded from the real harness running the script below |
| `<harness>.steps` | The steps `delegate status <id> --json` must show for it, in order: one per line, `kind<TAB>target<TAB>result` (result may be empty) |
| `<harness>.version` | The harness version it was recorded with |

Harnesses: `claude`, `codex`, `opencode`, `gemini`, `pi`. Re-record when a harness changes
its format; a missing recording fails S023 and P010 for that harness.

## The recording script
In the `setup_project` shop repo (`evals/lib/common.sh`), run the harness the way `delegate`
runs a work phase, in its streaming mode, with this prompt:

> Read shop/pricing.py and tests/test_pricing.py. Add `divide_bill(amount, people)` to
> shop/pricing.py that returns the amount split evenly, rounded to cents. Run
> `python3 -m unittest -q`. Then add a test for it to tests/test_pricing.py and run the tests
> again. Reply with one line saying what you did.

`<harness>.steps` is written by the lead from reading the stream, not by whoever writes the
adapter, so the adapter can't be tailored to it.
