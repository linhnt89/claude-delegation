"""Tests for the eval-private S146 attribution trace oracle.

These are tests of evaluation infrastructure, not user-level product verdicts and not
a live S146 result. They build synthetic traces with non-fixture pane/thread ids and
mutate one aspect per regression. The base trace passes; every mutation is expected to
move the verdict.
"""

from __future__ import annotations

import copy
import math
import os
import random
import sys
import unittest

sys.path.insert(
    0,
    os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"),
)

import codex_identity_oracle as oracle  # noqa: E402


SOURCE = "herdr:codex"
AGENT = "codex"
KIND = "id"

PANE_A = "pane-8f3c1a6d"
PANE_B = "pane-d94e2b70"
THREAD_A = "thread-71c9f04a"
THREAD_B = "thread-c83e5b26"
INSTANCE = "instance-5a2d9e13"
OTHER_INSTANCE = "instance-0b7f4c62"
FOREIGN_THREAD = "thread-3e6a8d51"

_UNSET = object()


def _ident(
    value: str,
    *,
    source: str = SOURCE,
    agent: str = AGENT,
    kind: str = KIND,
) -> dict:
    return {"source": source, "agent": agent, "kind": kind, "value": value}


def _times(start: float, end: float, step: float) -> list[float]:
    values = []
    index = 0
    while True:
        when = start + index * step
        if when > end:
            break
        values.append(when)
        index += 1
    if not values or values[-1] != end:
        values.append(end)
    return values


def _trace(
    *,
    a_launch: float = 0.0,
    a_reply: float = 2.0,
    a_exit: float = 13.0,
    b_launch: float = 16.0,
    b_reply: float = 18.0,
    end: float = 29.0,
    step: float = 0.5,
    pane_a: str = PANE_A,
    pane_b: str = PANE_B,
    thread_a: str = THREAD_A,
    thread_b: str = THREAD_B,
    instance: str = INSTANCE,
    a_post_exit: dict | None = None,
    daemon_start: float | None = None,
    b_start_identity=_UNSET,
) -> dict:
    samples = []
    for when in _times(a_launch, end, step):
        samples.append(
            {
                "time": when,
                "pane": pane_a,
                "ok": True,
                "identity": (
                    _ident(thread_a)
                    if when <= a_exit
                    else copy.deepcopy(a_post_exit)
                ),
            }
        )
    startup_identity = (
        _ident(thread_b)
        if b_start_identity is _UNSET
        else copy.deepcopy(b_start_identity)
    )
    for when in _times(b_launch, end, step):
        samples.append(
            {
                "time": when,
                "pane": pane_b,
                "ok": True,
                "identity": copy.deepcopy(startup_identity),
            }
        )
    samples.sort(key=lambda sample: (sample["time"], sample["pane"]))

    daemon_samples = [
        {"time": when, "ok": True, "instance": instance}
        for when in _times(
            a_reply if daemon_start is None else daemon_start, end, step
        )
    ]
    return {
        "version": 1,
        "panes": {"A": pane_a, "B": pane_b},
        "threads": {
            "A": {
                "id": thread_a,
                "source": SOURCE,
                "agent": AGENT,
                "kind": KIND,
                "started": a_launch,
                "completed": a_reply,
                "evidence_valid": True,
                "serving_instance": instance,
            },
            "B": {
                "id": thread_b,
                "source": SOURCE,
                "agent": AGENT,
                "kind": KIND,
                "started": b_launch,
                "completed": b_reply,
                "evidence_valid": True,
                "serving_instance": instance,
            },
        },
        "boundaries": {
            "a_launch": a_launch,
            "a_reply": a_reply,
            "a_exit": a_exit,
            "b_launch": b_launch,
            "b_reply": b_reply,
            "end": end,
        },
        "samples": samples,
        "daemon_samples": daemon_samples,
        "setup_valid": True,
        "cleanup_ok": True,
        "interrupted": False,
    }


def _find_sample(trace: dict, role: str, when: float) -> dict:
    pane = trace["panes"][role]
    for sample in trace["samples"]:
        if sample["pane"] == pane and sample["time"] == when:
            return sample
    raise AssertionError(f"no {role} sample at {when}")


def _find_daemon(trace: dict, when: float) -> dict:
    for sample in trace["daemon_samples"]:
        if sample["time"] == when:
            return sample
    raise AssertionError(f"no daemon sample at {when}")


def _remove_samples(trace: dict, role: str, start: float, end: float) -> None:
    pane = trace["panes"][role]
    trace["samples"] = [
        sample
        for sample in trace["samples"]
        if not (sample["pane"] == pane and start <= sample["time"] <= end)
    ]


def _remove_daemon(trace: dict, start: float, end: float) -> None:
    trace["daemon_samples"] = [
        sample
        for sample in trace["daemon_samples"]
        if not (start <= sample["time"] <= end)
    ]


class CodexIdentityOracleTest(unittest.TestCase):
    def assertVerdict(self, trace, verdict: str, codes=()):
        result = oracle.evaluate(trace)
        self.assertEqual(set(result), {"verdict", "reason", "issues"}, result)
        self.assertEqual(result["verdict"], verdict, result)
        self.assertIsInstance(result["issues"], list)
        for code in codes:
            self.assertIn(code, result["issues"], result)
        return result

    def test_valid_trace_passes_with_opaque_non_fixture_ids(self):
        trace = _trace()
        result = self.assertVerdict(trace, "pass")
        self.assertEqual(result["issues"], [])
        self.assertNotEqual(trace["panes"]["A"], trace["panes"]["B"])
        self.assertNotEqual(trace["threads"]["A"]["id"], trace["threads"]["B"]["id"])
        for pane in (trace["panes"]["A"], trace["panes"]["B"]):
            self.assertNotIn(pane, {"A", "B", "pane-a", "pane-b"})
        allowed = {trace["panes"]["A"], trace["panes"]["B"]}
        for sample in trace["samples"]:
            self.assertIn(sample["pane"], allowed)

        other = _trace(
            pane_a="opaque-alpha",
            pane_b="opaque-beta",
            thread_a="conversation-one",
            thread_b="conversation-two",
            instance="daemon-alpha",
        )
        self.assertVerdict(other, "pass")
        for sample in other["samples"]:
            self.assertIn(sample["pane"], {"opaque-alpha", "opaque-beta"})

    def test_result_is_deterministic_and_redacted(self):
        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = _ident(THREAD_A, source="other")
        before = copy.deepcopy(trace)
        first = oracle.evaluate(trace)
        second = oracle.evaluate(trace)
        self.assertEqual(first, second)
        self.assertEqual(trace, before)
        self.assertEqual(first["verdict"], "fail")
        text = repr(first)
        for value in (PANE_A, PANE_B, THREAD_A, THREAD_B, INSTANCE, FOREIGN_THREAD):
            self.assertNotIn(value, text)

    def test_evaluate_never_raises_on_hostile_shapes(self):
        for value in (None, True, 1, 1.0, "trace", [], {}, {"version": 1}, [1, 2, 3]):
            with self.subTest(value=repr(value)[:40]):
                result = oracle.evaluate(value)
                self.assertEqual(result["verdict"], "check_error", result)

        for mutate in (
            lambda trace: trace.__setitem__("samples", None),
            lambda trace: trace.__setitem__("samples", [None, 1, "x", []]),
            lambda trace: trace.__setitem__("daemon_samples", None),
            lambda trace: trace.__setitem__("daemon_samples", [None, 1, "x", []]),
        ):
            with self.subTest(mutate=mutate):
                trace = _trace()
                mutate(trace)
                result = oracle.evaluate(trace)
                self.assertEqual(result["verdict"], "check_error", result)

    def test_schema_and_field_types_are_strict(self):
        cases = (
            ("version-float", lambda trace: trace.__setitem__("version", 1.0)),
            ("version-bool", lambda trace: trace.__setitem__("version", True)),
            ("top-extra", lambda trace: trace.__setitem__("extra", 1)),
            ("panes-extra", lambda trace: trace["panes"].__setitem__("C", "pane-c")),
            (
                "sample-extra",
                lambda trace: trace["samples"][0].__setitem__("extra", 1),
            ),
            (
                "identity-extra",
                lambda trace: trace["samples"][0]["identity"].__setitem__("extra", "x"),
            ),
            ("time-string", lambda trace: trace["samples"][0].__setitem__("time", "0")),
            ("time-bool", lambda trace: trace["samples"][0].__setitem__("time", True)),
            ("pane-bad", lambda trace: trace["samples"][0].__setitem__("pane", "C")),
            ("ok-string", lambda trace: trace["samples"][0].__setitem__("ok", "false")),
            (
                "identity-list",
                lambda trace: trace["samples"][0].__setitem__("identity", []),
            ),
            (
                "identity-value-int",
                lambda trace: trace["samples"][0]["identity"].__setitem__("value", 1),
            ),
            (
                "daemon-extra",
                lambda trace: trace["daemon_samples"][0].__setitem__("extra", 1),
            ),
            (
                "daemon-instance-int",
                lambda trace: trace["daemon_samples"][0].__setitem__("instance", 5),
            ),
            (
                "daemon-instance-empty",
                lambda trace: trace["daemon_samples"][0].__setitem__("instance", ""),
            ),
            (
                "daemon-ok-string",
                lambda trace: trace["daemon_samples"][0].__setitem__("ok", "true"),
            ),
            (
                "setup-string",
                lambda trace: trace.__setitem__("setup_valid", "true"),
            ),
            (
                "cleanup-string",
                lambda trace: trace.__setitem__("cleanup_ok", "yes"),
            ),
            (
                "interrupted-string",
                lambda trace: trace.__setitem__("interrupted", "no"),
            ),
            (
                "thread-source",
                lambda trace: trace["threads"]["A"].__setitem__("source", "other"),
            ),
            (
                "thread-source-int",
                lambda trace: trace["threads"]["A"].__setitem__("source", 1),
            ),
            (
                "thread-started",
                lambda trace: trace["threads"]["A"].__setitem__("started", 0.5),
            ),
            (
                "thread-evidence-string",
                lambda trace: trace["threads"]["A"].__setitem__(
                    "evidence_valid", "true"
                ),
            ),
            (
                "thread-instance-empty",
                lambda trace: trace["threads"]["A"].__setitem__(
                    "serving_instance", ""
                ),
            ),
            (
                "boundary-order",
                lambda trace: trace["boundaries"].__setitem__("a_reply", 0.0),
            ),
            (
                "boundary-missing",
                lambda trace: trace["boundaries"].pop("end"),
            ),
            (
                "thread-missing",
                lambda trace: trace["threads"]["A"].pop("id"),
            ),
        )
        for name, mutate in cases:
            with self.subTest(name=name):
                trace = _trace()
                mutate(trace)
                result = oracle.evaluate(trace)
                self.assertEqual(result["verdict"], "check_error", result)

    def test_nonfinite_and_out_of_order_times_are_rejected(self):
        cases = (
            (
                "boundary-nan",
                lambda trace: trace["boundaries"].__setitem__("end", math.nan),
                "nonfinite_time",
            ),
            (
                "boundary-inf",
                lambda trace: trace["boundaries"].__setitem__("a_launch", -math.inf),
                "nonfinite_time",
            ),
            (
                "sample-nan",
                lambda trace: trace["samples"][0].__setitem__("time", math.nan),
                "nonfinite_time",
            ),
            (
                "daemon-inf",
                lambda trace: trace["daemon_samples"][0].__setitem__("time", math.inf),
                "nonfinite_time",
            ),
            (
                "sample-order",
                lambda trace: trace["samples"][1].__setitem__("time", -0.5),
                "out_of_order_samples",
            ),
            (
                "daemon-order",
                lambda trace: trace["daemon_samples"][1].__setitem__("time", 1.0),
                "out_of_order_samples",
            ),
        )
        for name, mutate, code in cases:
            with self.subTest(name=name):
                trace = _trace()
                mutate(trace)
                result = oracle.evaluate(trace)
                self.assertEqual(result["verdict"], "check_error", result)
                self.assertIn(code, result["issues"], result)

    def test_identity_bindings_are_distinct_and_complete(self):
        cases = (
            (
                "panes-equal",
                lambda trace: trace["panes"].__setitem__("B", trace["panes"]["A"]),
                "indistinct_identity",
            ),
            (
                "threads-equal",
                lambda trace: trace["threads"]["B"].__setitem__(
                    "id", trace["threads"]["A"]["id"]
                ),
                "indistinct_identity",
            ),
            (
                "pane-empty",
                lambda trace: trace["panes"].__setitem__("A", ""),
                "malformed_trace",
            ),
            (
                "thread-empty",
                lambda trace: trace["threads"]["A"].__setitem__("id", ""),
                "malformed_trace",
            ),
            (
                "instance-empty",
                lambda trace: trace["threads"]["A"].__setitem__(
                    "serving_instance", ""
                ),
                "malformed_trace",
            ),
        )
        for name, mutate, code in cases:
            with self.subTest(name=name):
                trace = _trace()
                mutate(trace)
                result = oracle.evaluate(trace)
                self.assertEqual(result["verdict"], "check_error", result)
                self.assertIn(code, result["issues"], result)

    def test_samples_are_bound_to_actual_opaque_pane_ids(self):
        trace = _trace()
        allowed = {trace["panes"]["A"], trace["panes"]["B"]}
        self.assertEqual(allowed, {PANE_A, PANE_B})
        for sample in trace["samples"]:
            self.assertIn(sample["pane"], allowed)
            self.assertNotIn(sample["pane"], {"A", "B"})

        for pane in ("A", "B", "pane-not-created", "", "pane-A"):
            with self.subTest(pane=pane):
                trace = _trace()
                trace["samples"][0]["pane"] = pane
                result = oracle.evaluate(trace)
                self.assertEqual(result["verdict"], "check_error", result)
                self.assertIn("unbound_pane", result["issues"], result)

        # Swapping only the actual mapping changes which role observed each
        # identity. The trace then attests B publishing A's full identity, so it
        # can never pass by falling back to role-looking sample labels.
        trace = _trace()
        trace["panes"]["A"], trace["panes"]["B"] = (
            trace["panes"]["B"],
            trace["panes"]["A"],
        )
        self.assertVerdict(trace, "fail", ["wrong_pane_identity"])

        for seed in range(24):
            rng = random.Random(4000 + seed)
            pane_a = "pane-%016x" % rng.getrandbits(64)
            pane_b = "pane-%016x" % rng.getrandbits(64)
            pane_b = pane_b if pane_b != pane_a else pane_b + "-b"
            trace = _trace(pane_a=pane_a, pane_b=pane_b)
            result = oracle.evaluate(trace)
            if result["verdict"] != "pass":
                self.fail(f"seed={seed} panes={pane_a},{pane_b}: {result}")
            for sample in trace["samples"]:
                self.assertIn(sample["pane"], {pane_a, pane_b})

            unknown = "pane-unknown-%016x" % rng.getrandbits(64)
            trace["samples"][rng.randrange(len(trace["samples"]))]["pane"] = unknown
            result = oracle.evaluate(trace)
            if result["verdict"] != "check_error":
                self.fail(f"seed={seed} unknown={unknown}: {result}")
            if "unbound_pane" not in result["issues"]:
                self.fail(f"seed={seed} unknown={unknown}: {result}")

    def test_setup_or_evidence_failure_prevents_product_verdict(self):
        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
        trace["setup_valid"] = False
        result = self.assertVerdict(trace, "check_error", ["setup_invalid"])
        self.assertNotIn("wrong_identity", result["issues"])

        for pane in ("A", "B"):
            with self.subTest(pane=pane):
                trace = _trace()
                _find_sample(trace, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
                trace["threads"][pane]["evidence_valid"] = False
                result = self.assertVerdict(
                    trace, "check_error", ["thread_evidence_invalid"]
                )
                self.assertNotIn("wrong_identity", result["issues"])

    def test_wrong_qualified_identity_after_deadline_fails(self):
        for field, value in (
            ("source", "other-source"),
            ("agent", "other-agent"),
            ("kind", "other-kind"),
            ("value", FOREIGN_THREAD),
        ):
            with self.subTest(field=field):
                trace = _trace()
                identity = _ident(THREAD_A)
                identity[field] = value
                _find_sample(trace, "A", 11.0)["identity"] = identity
                self.assertVerdict(trace, "fail", ["wrong_identity"])

        trace = _trace()
        _find_sample(trace, "B", 27.0)["identity"] = _ident(
            THREAD_B, agent="other-agent"
        )
        self.assertVerdict(trace, "fail", ["wrong_identity"])

        trace = _trace()
        _find_sample(trace, "A", 10.0)["identity"] = _ident(FOREIGN_THREAD)
        self.assertVerdict(trace, "fail", ["wrong_identity"])

    def test_unknown_after_deadline_fails_but_unknown_before_or_after_exit_does_not(
        self,
    ):
        for pane, when in (("A", 11.0), ("B", 27.0), ("A", 10.0), ("B", 26.0)):
            with self.subTest(pane=pane, when=when):
                trace = _trace()
                _find_sample(trace, pane, when)["identity"] = None
                self.assertVerdict(trace, "fail", ["unknown_after_deadline"])

        for pane, when in (("A", 9.5), ("B", 25.5)):
            with self.subTest(pane=pane, when=when):
                trace = _trace()
                _find_sample(trace, pane, when)["identity"] = None
                self.assertVerdict(trace, "pass")

        # A stops running at the observed a_exit boundary; B stays running
        # through the inclusive end boundary.
        for identity in (None, _ident(FOREIGN_THREAD)):
            trace = _trace()
            _find_sample(trace, "A", 13.0)["identity"] = identity
            self.assertVerdict(trace, "pass")
            trace = _trace()
            _find_sample(trace, "A", 29.0)["identity"] = identity
            self.assertVerdict(trace, "pass")

        trace = _trace()
        _find_sample(trace, "A", 12.5)["identity"] = None
        self.assertVerdict(trace, "fail", ["unknown_after_deadline"])

        trace = _trace()
        _find_sample(trace, "B", 29.0)["identity"] = None
        self.assertVerdict(trace, "fail", ["unknown_after_deadline"])

        self.assertVerdict(_trace(), "pass")

    def test_a_after_exit_may_retain_a_or_be_unknown(self):
        self.assertVerdict(_trace(), "pass")
        self.assertVerdict(_trace(a_post_exit=_ident(THREAD_A)), "pass")
        trace = _trace(a_post_exit=_ident(THREAD_B))
        self.assertVerdict(trace, "fail", ["wrong_pane_identity"])

    def test_wrong_pane_requires_the_full_qualified_identity(self):
        # The historical regression is a full source/agent/kind/value tuple,
        # not the opaque value under an unrelated namespace.
        for field in ("source", "agent", "kind"):
            with self.subTest(field=field):
                trace = _trace()
                identity = _ident(THREAD_A)
                identity[field] = "unrelated-namespace"
                _find_sample(trace, "B", 16.0)["identity"] = identity
                self.assertVerdict(trace, "pass")

                trace = _trace()
                identity = _ident(THREAD_B)
                identity[field] = "unrelated-namespace"
                _find_sample(trace, "A", 13.0)["identity"] = identity
                self.assertVerdict(trace, "pass")

        trace = _trace()
        _find_sample(trace, "A", 13.0)["identity"] = _ident(THREAD_B)
        self.assertVerdict(trace, "fail", ["wrong_pane_identity"])

        trace = _trace()
        _find_sample(trace, "A", 13.5)["identity"] = _ident(THREAD_B)
        self.assertVerdict(trace, "fail", ["wrong_pane_identity"])

        trace = _trace()
        _find_sample(trace, "B", 16.0)["identity"] = _ident(THREAD_A)
        self.assertVerdict(trace, "fail", ["wrong_pane_identity"])

        # After the deadline the colliding value is foreign/mismatched, which
        # is a post-deadline identity failure rather than wrong-pane evidence.
        trace = _trace()
        identity = _ident(THREAD_A, source="unrelated-namespace")
        _find_sample(trace, "B", 26.0)["identity"] = identity
        result = self.assertVerdict(trace, "fail", ["wrong_identity"])
        self.assertNotIn("wrong_pane_identity", result["issues"], result)

    def test_wrong_pane_transients_during_b_startup_fail(self):
        for pane, when, other in (
            ("A", 13.0, THREAD_B),
            ("A", 13.5, THREAD_B),
            ("A", 16.0, THREAD_B),
            ("B", 16.0, THREAD_A),
            ("B", 16.5, THREAD_A),
        ):
            with self.subTest(pane=pane, when=when):
                trace = _trace()
                _find_sample(trace, pane, when)["identity"] = _ident(other)
                self.assertVerdict(trace, "fail", ["wrong_pane_identity"])

        # A foreign startup report that is not A's own qualified identity is
        # still within grace and is not a wrong-pane publication.
        trace = _trace()
        _find_sample(trace, "B", 16.0)["identity"] = _ident(
            FOREIGN_THREAD, agent="other-agent"
        )
        self.assertVerdict(trace, "pass")

    def test_post_completion_missing_or_unknown_is_caught(self):
        trace = _trace()
        _remove_samples(trace, "A", 2.0, 13.0)
        self.assertVerdict(
            trace, "check_error", ["coverage_gap", "missing_post_reply_sample"]
        )

        trace = _trace()
        _remove_samples(trace, "B", 18.0, 29.0)
        self.assertVerdict(
            trace, "check_error", ["coverage_gap", "missing_post_reply_sample"]
        )

        trace = _trace()
        _remove_samples(trace, "A", 10.0, 13.0)
        self.assertVerdict(trace, "check_error", ["coverage_gap"])

        trace = _trace()
        _remove_samples(trace, "B", 26.0, 29.0)
        self.assertVerdict(trace, "check_error", ["coverage_gap"])

    def test_a_post_reply_sample_at_exit_does_not_count_as_running(self):
        trace = _trace()
        for when in _times(2.0, 12.5, 0.5):
            sample = _find_sample(trace, "A", when)
            sample["ok"] = False
            sample["identity"] = None
        result = self.assertVerdict(
            trace,
            "check_error",
            ["failed_read", "missing_post_reply_sample"],
        )
        self.assertNotIn("unknown_after_deadline", result["issues"], result)

        # The sample at exactly a_exit is still required for coverage and may be
        # successful, it just cannot satisfy A's post-reply running requirement.
        self.assertIs(_find_sample(trace, "A", 13.0)["ok"], True)

    def test_startup_transition_and_endpoint_gaps_are_caught(self):
        for name, pane, start, end in (
            ("a-start", "A", 0.0, 1.5),
            ("a-transition", "A", 13.5, 16.0),
            ("b-start", "B", 16.0, 18.0),
            ("a-end", "A", 27.5, 29.0),
            ("b-end", "B", 27.5, 29.0),
        ):
            with self.subTest(name=name):
                trace = _trace()
                _remove_samples(trace, pane, start, end)
                self.assertVerdict(trace, "check_error", ["coverage_gap"])

        for name, start, end in (
            ("daemon-start", 2.0, 3.5),
            ("daemon-end", 27.5, 29.0),
        ):
            with self.subTest(name=name):
                trace = _trace()
                _remove_daemon(trace, start, end)
                self.assertVerdict(trace, "check_error", ["coverage_gap"])

    def test_failed_reads_are_not_skipped_but_do_not_manufacture_failure(self):
        trace = _trace()
        sample = _find_sample(trace, "A", 11.0)
        sample["ok"] = False
        sample["identity"] = None
        self.assertVerdict(trace, "check_error", ["failed_read"])

        trace = _trace()
        sample = _find_sample(trace, "B", 27.0)
        sample["ok"] = False
        sample["identity"] = None
        self.assertVerdict(trace, "check_error", ["failed_read"])

        trace = _trace()
        daemon = _find_daemon(trace, 10.0)
        daemon["ok"] = False
        daemon["instance"] = None
        self.assertVerdict(trace, "check_error", ["failed_daemon_read"])

        trace = _trace(daemon_start=0.0)
        daemon = _find_daemon(trace, 1.0)
        daemon["ok"] = False
        daemon["instance"] = None
        self.assertVerdict(trace, "pass")

        trace = _trace()
        sample = _find_sample(trace, "A", 11.0)
        sample["ok"] = False
        sample["identity"] = _ident(THREAD_B)
        result = self.assertVerdict(trace, "check_error", ["failed_read"])
        self.assertNotIn("wrong_pane_identity", result["issues"])

        trace = _trace()
        sample = _find_sample(trace, "A", 11.0)
        sample["ok"] = False
        sample["identity"] = None
        _find_sample(trace, "B", 27.0)["identity"] = _ident(FOREIGN_THREAD)
        self.assertVerdict(trace, "fail", ["failed_read", "wrong_identity"])

        # The same counterexample must win when it appears before the failed read.
        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
        sample = _find_sample(trace, "B", 27.0)
        sample["ok"] = False
        sample["identity"] = None
        self.assertVerdict(trace, "fail", ["failed_read", "wrong_identity"])

    def test_unknown_is_not_a_failed_read(self):
        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = None
        result = self.assertVerdict(trace, "fail", ["unknown_after_deadline"])
        self.assertNotIn("failed_read", result["issues"])

        trace = _trace()
        sample = _find_sample(trace, "A", 11.0)
        sample["ok"] = False
        sample["identity"] = None
        result = self.assertVerdict(trace, "check_error", ["failed_read"])
        self.assertNotIn("unknown_after_deadline", result["issues"])

    def test_daemon_continuity_and_serving_instance_proof(self):
        trace = _trace()
        _find_daemon(trace, 10.0)["instance"] = OTHER_INSTANCE
        self.assertVerdict(
            trace,
            "check_error",
            ["daemon_instance_changed", "serving_instance_unbound"],
        )

        trace = _trace()
        trace["threads"]["A"]["serving_instance"] = OTHER_INSTANCE
        trace["threads"]["B"]["serving_instance"] = OTHER_INSTANCE
        self.assertVerdict(
            trace, "check_error", ["serving_instance_unbound"]
        )

        trace = _trace()
        trace["threads"]["B"]["serving_instance"] = OTHER_INSTANCE
        self.assertVerdict(
            trace,
            "check_error",
            ["serving_instance_mismatch", "serving_instance_unbound"],
        )

        trace = _trace()
        trace["daemon_samples"] = []
        self.assertVerdict(
            trace, "check_error", ["missing_coverage", "daemon_missing"]
        )

        trace = _trace()
        daemon = _find_daemon(trace, 10.0)
        daemon["ok"] = False
        daemon["instance"] = None
        self.assertVerdict(trace, "check_error", ["failed_daemon_read"])

        trace = _trace()
        _find_daemon(trace, 10.0)["instance"] = OTHER_INSTANCE
        _find_sample(trace, "B", 27.0)["identity"] = _ident(FOREIGN_THREAD)
        self.assertVerdict(
            trace, "fail", ["daemon_instance_changed", "wrong_identity"]
        )

    def test_fail_precedence_retains_coverage_issues(self):
        trace = _trace()
        _remove_samples(trace, "A", 0.0, 1.5)
        _find_sample(trace, "B", 27.0)["identity"] = _ident(FOREIGN_THREAD)
        self.assertVerdict(trace, "fail", ["coverage_gap", "wrong_identity"])

        trace = _trace(a_exit=12.99)
        _find_sample(trace, "B", 27.0)["identity"] = _ident(FOREIGN_THREAD)
        self.assertVerdict(trace, "fail", ["dwell_too_short", "wrong_identity"])

    def test_cleanup_and_interruption_are_check_errors_unless_failure_exists(self):
        trace = _trace()
        trace["cleanup_ok"] = False
        self.assertVerdict(trace, "check_error", ["cleanup_failed"])

        trace = _trace()
        trace["interrupted"] = True
        self.assertVerdict(trace, "check_error", ["interrupted"])

        trace = _trace()
        trace["cleanup_ok"] = False
        trace["interrupted"] = True
        self.assertVerdict(trace, "check_error", ["cleanup_failed", "interrupted"])

        trace = _trace()
        trace["cleanup_ok"] = False
        trace["interrupted"] = True
        _find_sample(trace, "B", 27.0)["identity"] = _ident(FOREIGN_THREAD)
        self.assertVerdict(
            trace,
            "fail",
            ["cleanup_failed", "interrupted", "wrong_identity"],
        )

    def test_contradictory_evidence_is_check_error(self):
        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
        trace["threads"]["A"]["started"] = 0.5
        self.assertVerdict(trace, "check_error", ["inconsistent_binding"])

        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
        trace["boundaries"]["a_reply"] = trace["boundaries"]["a_launch"]
        self.assertVerdict(trace, "check_error", ["inconsistent_binding"])

        trace = _trace()
        _find_sample(trace, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
        trace["threads"]["B"]["id"] = trace["threads"]["A"]["id"]
        self.assertVerdict(trace, "check_error", ["indistinct_identity"])

    def test_multiple_calls_do_not_leak_state(self):
        good = _trace()
        bad = _trace()
        _find_sample(bad, "A", 11.0)["identity"] = _ident(FOREIGN_THREAD)
        error = _trace()
        error["cleanup_ok"] = False
        self.assertVerdict(good, "pass")
        self.assertVerdict(bad, "fail")
        self.assertVerdict(error, "check_error")
        self.assertVerdict(good, "pass")
        self.assertVerdict(bad, "fail")

    def test_empty_slices_are_not_success(self):
        trace = _trace()
        trace["samples"] = []
        self.assertVerdict(trace, "check_error", ["missing_coverage"])

        trace = _trace()
        trace["daemon_samples"] = []
        self.assertVerdict(
            trace, "check_error", ["missing_coverage", "daemon_missing"]
        )

        trace = _trace()
        _remove_samples(trace, "A", 0.0, 29.0)
        self.assertVerdict(trace, "check_error", ["missing_coverage"])

        trace = _trace()
        _remove_samples(trace, "B", 16.0, 29.0)
        self.assertVerdict(trace, "check_error", ["missing_coverage"])

    def test_max_gap_and_edges_are_inclusive(self):
        self.assertVerdict(_trace(step=1.0), "pass")

        trace = _trace(step=1.0)
        _remove_samples(trace, "A", 0.0, 0.0)
        self.assertVerdict(trace, "pass")

        trace = _trace(step=1.0)
        _remove_samples(trace, "A", 0.0, 1.0)
        self.assertVerdict(trace, "check_error", ["coverage_gap"])

        trace = _trace(step=1.0)
        _remove_samples(trace, "A", 29.0, 29.0)
        self.assertVerdict(trace, "pass")

        trace = _trace(step=1.0)
        _remove_samples(trace, "A", 28.0, 29.0)
        self.assertVerdict(trace, "check_error", ["coverage_gap"])

    def test_seeded_grace_boundary(self):
        for seed in range(12):
            rng = random.Random(seed)
            epsilon = rng.uniform(0.01, 0.49)
            with self.subTest(seed=seed):
                trace = _trace()
                sample = _find_sample(trace, "A", 9.5)
                sample["time"] = 10.0 - epsilon
                sample["identity"] = _ident(FOREIGN_THREAD)
                trace["samples"].sort(key=lambda item: (item["time"], item["pane"]))
                result = oracle.evaluate(trace)
                if result["verdict"] != "pass":
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

                trace = _trace()
                sample = _find_sample(trace, "B", 25.5)
                sample["time"] = 26.0 - epsilon
                sample["identity"] = _ident(FOREIGN_THREAD)
                trace["samples"].sort(key=lambda item: (item["time"], item["pane"]))
                result = oracle.evaluate(trace)
                if result["verdict"] != "pass":
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

                trace = _trace()
                sample = _find_sample(trace, "A", 9.5)
                sample["time"] = 10.0 - epsilon
                sample["identity"] = None
                trace["samples"].sort(key=lambda item: (item["time"], item["pane"]))
                result = oracle.evaluate(trace)
                if result["verdict"] != "pass":
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

                trace = _trace()
                sample = _find_sample(trace, "B", 25.5)
                sample["time"] = 26.0 - epsilon
                sample["identity"] = None
                trace["samples"].sort(key=lambda item: (item["time"], item["pane"]))
                result = oracle.evaluate(trace)
                if result["verdict"] != "pass":
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

                for pane, when in (("A", 10.0), ("B", 26.0)):
                    trace = _trace()
                    _find_sample(trace, pane, when)["identity"] = _ident(
                        FOREIGN_THREAD
                    )
                    result = oracle.evaluate(trace)
                    if result["verdict"] != "fail":
                        self.fail(f"seed={seed} {pane}@{when}: {result}")

    def test_seeded_dwell_boundary(self):
        for seed in range(12):
            rng = random.Random(1000 + seed)
            epsilon = rng.uniform(0.01, 0.49)
            with self.subTest(seed=seed):
                result = oracle.evaluate(_trace())
                if result["verdict"] != "pass":
                    self.fail(f"seed={seed}: {result}")

                trace = _trace(a_exit=13.0 - epsilon)
                result = oracle.evaluate(trace)
                if (
                    result["verdict"] != "check_error"
                    or "dwell_too_short" not in result["issues"]
                ):
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

                trace = _trace(end=29.0 - epsilon)
                result = oracle.evaluate(trace)
                if (
                    result["verdict"] != "check_error"
                    or "dwell_too_short" not in result["issues"]
                ):
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

    def test_seeded_gap_boundary(self):
        for seed in range(12):
            epsilon = 0.01 + (seed % 9) * 0.01
            with self.subTest(seed=seed):
                result = oracle.evaluate(_trace(step=1.0))
                if result["verdict"] != "pass":
                    self.fail(f"seed={seed}: {result}")

                trace = _trace(step=1.0 + epsilon)
                result = oracle.evaluate(trace)
                if (
                    result["verdict"] != "check_error"
                    or "coverage_gap" not in result["issues"]
                ):
                    self.fail(f"seed={seed} epsilon={epsilon}: {result}")

    def test_seeded_qualified_namespace_collisions(self):
        for seed in range(24):
            rng = random.Random(7000 + seed)
            field = rng.choice(("source", "agent", "kind"))
            role, when, other = rng.choice(
                (
                    ("A", 13.0, THREAD_B),
                    ("A", 13.5, THREAD_B),
                    ("B", 16.0, THREAD_A),
                    ("B", 16.5, THREAD_A),
                )
            )
            trace = _trace()
            identity = _ident(other)
            identity[field] = f"foreign-{seed}-{rng.getrandbits(16):x}"
            _find_sample(trace, role, when)["identity"] = identity
            result = oracle.evaluate(trace)
            if result["verdict"] != "pass":
                self.fail(f"seed={seed} role={role} when={when}: {result}")

            trace = _trace()
            _find_sample(trace, role, when)["identity"] = _ident(other)
            result = oracle.evaluate(trace)
            if result["verdict"] != "fail":
                self.fail(f"seed={seed} role={role} when={when}: {result}")
            if "wrong_pane_identity" not in result["issues"]:
                self.fail(f"seed={seed} role={role} when={when}: {result}")

    def test_seeded_lifecycle_boundaries(self):
        for seed in range(24):
            rng = random.Random(9000 + seed)
            epsilon = rng.uniform(0.01, 0.49)

            trace = _trace()
            sample = _find_sample(trace, "A", 13.0)
            sample["time"] = 13.0 - epsilon
            sample["identity"] = None
            trace["samples"].sort(key=lambda item: (item["time"], item["pane"]))
            result = oracle.evaluate(trace)
            if result["verdict"] != "fail":
                self.fail(f"seed={seed} epsilon={epsilon}: {result}")

            trace = _trace()
            _find_sample(trace, "A", 13.0)["identity"] = None
            result = oracle.evaluate(trace)
            if result["verdict"] != "pass":
                self.fail(f"seed={seed} epsilon={epsilon}: {result}")

            trace = _trace()
            _find_sample(trace, "B", 29.0)["identity"] = None
            result = oracle.evaluate(trace)
            if result["verdict"] != "fail":
                self.fail(f"seed={seed} epsilon={epsilon}: {result}")

            trace = _trace()
            _find_sample(trace, "A", 29.0)["identity"] = None
            result = oracle.evaluate(trace)
            if result["verdict"] != "pass":
                self.fail(f"seed={seed} epsilon={epsilon}: {result}")


if __name__ == "__main__":
    unittest.main()
