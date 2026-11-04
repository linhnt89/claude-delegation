"""Pure, fail-closed oracle for the eval-private S146 attribution trace (v1).

The live driver owns resources, redaction and independent evidence collection. This
module only judges a complete in-memory trace. It imports no process, environment,
file or network APIs, has no import-time side effects and keeps no mutable module-level
state.

``evaluate(trace)`` returns exactly ``{"verdict", "reason", "issues"}``. Reason text
and issue codes never contain trace values such as pane ids, thread ids, instance
tokens or paths.

Samples carry the actual opaque pane ids from ``trace["panes"]``; the oracle maps
them back to the observer roles A and B and never accepts role labels as a fallback.
A is observed running on ``[a_launch, a_exit)`` while B is running through the
inclusive final ``end`` boundary.
"""

from __future__ import annotations

import math

TRACE_VERSION = 1
GRACE_SECONDS = 10.0
DWELL_SECONDS = 3.0
MAX_GAP_SECONDS = 1.0

SOURCE = "herdr:codex"
AGENT = "codex"
KIND = "id"

PANES = ("A", "B")
BOUNDARY_KEYS = ("a_launch", "a_reply", "a_exit", "b_launch", "b_reply", "end")
THREAD_KEYS = (
    "id",
    "source",
    "agent",
    "kind",
    "started",
    "completed",
    "evidence_valid",
    "serving_instance",
)
IDENTITY_KEYS = ("source", "agent", "kind", "value")
SAMPLE_KEYS = ("time", "pane", "ok", "identity")
DAEMON_KEYS = ("time", "ok", "instance")
TRACE_KEYS = (
    "version",
    "panes",
    "threads",
    "boundaries",
    "samples",
    "daemon_samples",
    "setup_valid",
    "cleanup_ok",
    "interrupted",
)

_PASS_REASON = "all required attribution, coverage and daemon conditions hold"
_FAIL_REASON = "concrete attribution failure observed"
_CHECK_ERROR_REASON = "trace cannot prove required attribution"
_MALFORMED_REASON = "trace is malformed or incomplete"
_SETUP_REASON = "trace setup is not valid"
_EVIDENCE_REASON = "thread evidence is incomplete"


class _Malformed(Exception):
    """A trace row cannot be trusted; ``evaluate`` maps it to check_error."""

    def __init__(self, code: str = "malformed_trace") -> None:
        super().__init__(code)
        self.code = code


def _result(verdict: str, reason: str, issues) -> dict:
    return {"verdict": verdict, "reason": reason, "issues": sorted(set(issues))}


def _mapping(value, code: str = "malformed_trace") -> dict:
    if not isinstance(value, dict):
        raise _Malformed(code)
    return value


def _exact_keys(value: dict, keys, code: str = "malformed_trace") -> None:
    if set(value) != set(keys):
        raise _Malformed(code)


def _string(value, *, nonempty: bool = False) -> str:
    if not isinstance(value, str):
        raise _Malformed("malformed_trace")
    result = str(value)
    if nonempty and not result:
        raise _Malformed("malformed_trace")
    return result


def _boolean(value) -> bool:
    if type(value) is not bool:
        raise _Malformed("malformed_trace")
    return value


def _number(value) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise _Malformed("malformed_trace")
    number = float(value)
    if not math.isfinite(number):
        raise _Malformed("nonfinite_time")
    return number


def _identity(value):
    if value is None:
        return None
    raw = _mapping(value)
    _exact_keys(raw, IDENTITY_KEYS)
    return {
        "source": _string(raw["source"]),
        "agent": _string(raw["agent"]),
        "kind": _string(raw["kind"]),
        "value": _string(raw["value"]),
    }


def _validate(trace) -> dict:
    root = _mapping(trace)
    _exact_keys(root, TRACE_KEYS)

    version = root["version"]
    if type(version) is not int or version != TRACE_VERSION:
        raise _Malformed("malformed_trace")
    setup_valid = _boolean(root["setup_valid"])
    cleanup_ok = _boolean(root["cleanup_ok"])
    interrupted = _boolean(root["interrupted"])

    raw_panes = _mapping(root["panes"])
    _exact_keys(raw_panes, PANES)
    pane_a = _string(raw_panes["A"], nonempty=True)
    pane_b = _string(raw_panes["B"], nonempty=True)
    if pane_a == pane_b:
        raise _Malformed("indistinct_identity")

    raw_threads = _mapping(root["threads"])
    _exact_keys(raw_threads, PANES)
    threads = {}
    for pane in PANES:
        raw = _mapping(raw_threads[pane])
        _exact_keys(raw, THREAD_KEYS)
        source = _string(raw["source"])
        agent = _string(raw["agent"])
        kind = _string(raw["kind"])
        if (source, agent, kind) != (SOURCE, AGENT, KIND):
            raise _Malformed("inconsistent_binding")
        threads[pane] = {
            "id": _string(raw["id"], nonempty=True),
            "started": _number(raw["started"]),
            "completed": _number(raw["completed"]),
            "evidence_valid": _boolean(raw["evidence_valid"]),
            "serving_instance": _string(raw["serving_instance"], nonempty=True),
        }
    if threads["A"]["id"] == threads["B"]["id"]:
        raise _Malformed("indistinct_identity")

    raw_bounds = _mapping(root["boundaries"])
    _exact_keys(raw_bounds, BOUNDARY_KEYS)
    bounds = {key: _number(raw_bounds[key]) for key in BOUNDARY_KEYS}
    if not (
        bounds["a_launch"]
        < bounds["a_reply"]
        < bounds["a_exit"]
        < bounds["b_launch"]
        < bounds["b_reply"]
        < bounds["end"]
    ):
        raise _Malformed("inconsistent_binding")
    if (
        threads["A"]["started"] != bounds["a_launch"]
        or threads["A"]["completed"] != bounds["a_reply"]
        or threads["B"]["started"] != bounds["b_launch"]
        or threads["B"]["completed"] != bounds["b_reply"]
    ):
        raise _Malformed("inconsistent_binding")

    return {
        "panes": {"A": pane_a, "B": pane_b},
        "threads": threads,
        "bounds": bounds,
        "samples": _validate_samples(
            root["samples"], {pane_a: "A", pane_b: "B"}
        ),
        "daemon": _validate_daemon_samples(root["daemon_samples"]),
        "setup_valid": setup_valid,
        "cleanup_ok": cleanup_ok,
        "interrupted": interrupted,
    }


def _validate_samples(raw_samples, pane_roles: dict) -> list:
    if not isinstance(raw_samples, list):
        raise _Malformed("malformed_trace")
    samples = []
    previous = None
    for entry in raw_samples:
        raw = _mapping(entry)
        _exact_keys(raw, SAMPLE_KEYS)
        time = _number(raw["time"])
        pane = _string(raw["pane"])
        role = pane_roles.get(pane)
        if role is None:
            # Only the actual opaque trace pane ids are bound. Role labels and
            # unknown strings are malformed, never silently mapped by name.
            raise _Malformed("unbound_pane")
        ok = _boolean(raw["ok"])
        identity = _identity(raw["identity"])
        if previous is not None and time < previous:
            raise _Malformed("out_of_order_samples")
        previous = time
        samples.append(
            {"time": time, "role": role, "ok": ok, "identity": identity}
        )
    return samples


def _validate_daemon_samples(raw_samples) -> list:
    if not isinstance(raw_samples, list):
        raise _Malformed("malformed_trace")
    samples = []
    previous = None
    for entry in raw_samples:
        raw = _mapping(entry)
        _exact_keys(raw, DAEMON_KEYS)
        time = _number(raw["time"])
        ok = _boolean(raw["ok"])
        raw_instance = raw["instance"]
        if raw_instance is None:
            instance = None
        else:
            instance = _string(raw_instance)
            if not instance:
                raise _Malformed("empty_instance")
        if ok and instance is None:
            raise _Malformed("empty_instance")
        if previous is not None and time < previous:
            raise _Malformed("out_of_order_samples")
        previous = time
        samples.append({"time": time, "ok": ok, "instance": instance})
    return samples


def _check_coverage(times, start: float, end: float, issues: set) -> None:
    window = sorted(when for when in times if start <= when <= end)
    if not window:
        issues.add("missing_coverage")
        return
    if window[0] - start > MAX_GAP_SECONDS:
        issues.add("coverage_gap")
    for previous, current in zip(window, window[1:]):
        if current - previous > MAX_GAP_SECONDS:
            issues.add("coverage_gap")
    if end - window[-1] > MAX_GAP_SECONDS:
        issues.add("coverage_gap")


def _identity_tuple(identity):
    return (
        identity["source"],
        identity["agent"],
        identity["kind"],
        identity["value"],
    )


def _check_identities(parsed: dict, counterexamples: set) -> None:
    bounds = parsed["bounds"]
    threads = parsed["threads"]
    expected = {
        "A": (SOURCE, AGENT, KIND, threads["A"]["id"]),
        "B": (SOURCE, AGENT, KIND, threads["B"]["id"]),
    }
    starts = {
        "A": bounds["a_launch"] + GRACE_SECONDS,
        "B": bounds["b_launch"] + GRACE_SECONDS,
    }

    for sample in parsed["samples"]:
        if not sample["ok"]:
            # A failed read is never a counterexample, even if it carries a
            # well-formed identity; that identity is untrusted.
            continue
        role = sample["role"]
        when = sample["time"]
        identity = sample["identity"]

        # The historical wrong-pane regression: A's exited pane publishing B's
        # full qualified identity, and B publishing A's full qualified identity
        # during B's phase, are failures even inside a startup grace. Foreign
        # namespaces that merely collide on the opaque value are not this
        # counterexample.
        if identity is not None and _identity_tuple(identity) == expected[
            "B" if role == "A" else "A"
        ]:
            if role == "A" and bounds["a_exit"] <= when <= bounds["end"]:
                counterexamples.add("wrong_pane_identity")
            if role == "B" and bounds["b_launch"] <= when <= bounds["end"]:
                counterexamples.add("wrong_pane_identity")

        # After the reporting deadline a running TUI must report exactly its own
        # qualified identity. Unknown before the deadline, and after A exits, is
        # not counterevidence. A runs on [a_launch, a_exit); B runs through the
        # inclusive end boundary. Checking from launch + grace (rather than from
        # max(reply, launch + grace)) also enforces the separate rule that a
        # foreign/mismatched identity after grace is failure.
        if when < starts[role]:
            continue
        running = when < bounds["a_exit"] if role == "A" else when <= bounds["end"]
        if not running:
            # A is not running after its observed exit. Only publishing B's
            # full qualified identity there is the reproduced counterexample;
            # other post-exit values are not inferred as product failures.
            continue
        if identity is None:
            counterexamples.add("unknown_after_deadline")
        elif _identity_tuple(identity) != expected[role]:
            counterexamples.add("wrong_identity")


def _check_daemon(parsed: dict, issues: set) -> None:
    bounds = parsed["bounds"]
    threads = parsed["threads"]
    token_a = threads["A"]["serving_instance"]
    token_b = threads["B"]["serving_instance"]
    if token_a != token_b:
        issues.add("serving_instance_mismatch")

    window = [
        sample
        for sample in parsed["daemon"]
        if bounds["a_reply"] <= sample["time"] <= bounds["end"]
    ]
    successful = [sample for sample in window if sample["ok"]]
    if not successful:
        issues.add("daemon_missing")
        return
    instances = {sample["instance"] for sample in successful}
    if len(instances) > 1:
        issues.add("daemon_instance_changed")
    if any(instance != token_a or instance != token_b for instance in instances):
        issues.add("serving_instance_unbound")


def _judge(parsed: dict):
    issues = set()
    counterexamples = set()
    bounds = parsed["bounds"]
    samples = parsed["samples"]
    daemon = parsed["daemon"]

    if (
        bounds["a_exit"]
        < max(bounds["a_reply"], bounds["a_launch"] + GRACE_SECONDS)
        + DWELL_SECONDS
    ):
        issues.add("dwell_too_short")
    if (
        bounds["end"]
        < max(bounds["b_reply"], bounds["b_launch"] + GRACE_SECONDS)
        + DWELL_SECONDS
    ):
        issues.add("dwell_too_short")

    _check_coverage(
        [sample["time"] for sample in samples if sample["role"] == "A"],
        bounds["a_launch"],
        bounds["end"],
        issues,
    )
    _check_coverage(
        [sample["time"] for sample in samples if sample["role"] == "B"],
        bounds["b_launch"],
        bounds["end"],
        issues,
    )
    _check_coverage(
        [sample["time"] for sample in daemon],
        bounds["a_reply"],
        bounds["end"],
        issues,
    )

    for sample in samples:
        if sample["ok"]:
            continue
        if sample["role"] == "A" and bounds["a_launch"] <= sample["time"] <= bounds["end"]:
            issues.add("failed_read")
        elif sample["role"] == "B" and bounds["b_launch"] <= sample["time"] <= bounds["end"]:
            issues.add("failed_read")
    for sample in daemon:
        if not sample["ok"] and bounds["a_reply"] <= sample["time"] <= bounds["end"]:
            issues.add("failed_daemon_read")

    if not any(
        sample["ok"]
        and sample["role"] == "A"
        and bounds["a_reply"] <= sample["time"] < bounds["a_exit"]
        for sample in samples
    ):
        issues.add("missing_post_reply_sample")
    if not any(
        sample["ok"]
        and sample["role"] == "B"
        and bounds["b_reply"] <= sample["time"] <= bounds["end"]
        for sample in samples
    ):
        issues.add("missing_post_reply_sample")

    _check_identities(parsed, counterexamples)
    _check_daemon(parsed, issues)

    if not parsed["cleanup_ok"]:
        issues.add("cleanup_failed")
    if parsed["interrupted"]:
        issues.add("interrupted")

    return issues, counterexamples


def evaluate(trace: dict) -> dict:
    """Judge a complete trace. Malformed input returns check_error, never a traceback."""
    try:
        parsed = _validate(trace)
    except _Malformed as error:
        return _result("check_error", _MALFORMED_REASON, [error.code])
    except Exception:
        return _result("check_error", _MALFORMED_REASON, ["malformed_trace"])

    # False setup/evidence flags prevent product verdicts, even with a wrong sample.
    if not parsed["setup_valid"]:
        return _result("check_error", _SETUP_REASON, ["setup_invalid"])
    if (
        not parsed["threads"]["A"]["evidence_valid"]
        or not parsed["threads"]["B"]["evidence_valid"]
    ):
        return _result("check_error", _EVIDENCE_REASON, ["thread_evidence_invalid"])

    try:
        issues, counterexamples = _judge(parsed)
    except Exception:
        return _result("check_error", _MALFORMED_REASON, ["malformed_trace"])

    if counterexamples:
        return _result("fail", _FAIL_REASON, counterexamples | issues)
    if issues:
        return _result("check_error", _CHECK_ERROR_REASON, issues)
    return _result("pass", _PASS_REASON, [])
