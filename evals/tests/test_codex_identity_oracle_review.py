"""Adversarial regression evidence; synthetic traces, not a live S146 verdict."""

import unittest

import test_codex_identity_oracle as fixtures


class QualifiedWrongPaneReviewTest(unittest.TestCase):
    def test_startup_value_collision_is_not_qualified_wrong_pane_identity(self):
        self.assertEqual(fixtures.oracle.evaluate(fixtures._trace())["verdict"], "pass")
        for field in ("source", "agent", "kind"):
            with self.subTest(field=field):
                trace = fixtures._trace()
                identity = fixtures._ident(fixtures.THREAD_A)
                identity[field] = "unrelated-namespace"
                fixtures._find_sample(trace, "B", 16.0)["identity"] = identity
                # B is still inside reporting grace. Its report is foreign, but
                # not A's qualified identity: only the opaque value collides.
                result = fixtures.oracle.evaluate(trace)
                self.assertEqual(result["verdict"], "pass", result)


if __name__ == "__main__":
    unittest.main()
