#!/usr/bin/env python3
"""Negative controls for the relevance gate, without changing on-disk fixtures."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("gate", Path(__file__).with_name("search-relevance.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class GateTests(unittest.TestCase):
    report = None
    fixture = json.loads(Path("crates/matric-search/tests/fixtures/golden_test_set.json").read_text())
    baseline = json.loads(Path("crates/matric-search/tests/fixtures/keyword_relevance_baseline.json").read_text())

    def validate(self, report, baseline=None):
        gate.validate(report, self.fixture, baseline or self.baseline)

    def mutated_ranking(self, mutate):
        report = copy.deepcopy(self.report)
        run = next(r for r in report["runs"] if r["mode"] == "adaptive")
        row = next(q for q in run["queries"] if q["id"] == "q5")
        mutate(row["ranking"])
        qrels = next(q["relevant_notes"] for q in self.fixture["queries"] if q["id"] == "q5")
        row["metrics"] = gate.metrics(row["ranking"], qrels)
        run["metrics"] = {key: sum(q["metrics"][key] for q in run["queries"]) / 8 for key in gate.METRICS}
        run["zero_result_rate"] = sum(not q["ranking"] for q in run["queries"]) / 8
        return report

    def test_accepted_report(self):
        self.validate(self.report)

    def test_removed_relevant_hit(self):
        with self.assertRaisesRegex(ValueError, "relevance regressions"):
            self.validate(self.mutated_ranking(lambda ranking: ranking.pop(0)))

    def test_ranking_inversion(self):
        def invert(ranking):
            ranking[0], ranking[-1] = ranking[-1], ranking[0]
        with self.assertRaises(ValueError):
            self.validate(self.mutated_ranking(invert))

    def test_missing_query(self):
        report = copy.deepcopy(self.report)
        report["runs"][0]["queries"].pop()
        with self.assertRaisesRegex(ValueError, "missing/duplicate query"):
            self.validate(report)

    def test_mislabeled_effective_mode(self):
        report = copy.deepcopy(self.report)
        report["runs"][0]["effective_mode"] = "semantic"
        with self.assertRaisesRegex(ValueError, "masquerade"):
            self.validate(report)

    def test_degradation(self):
        report = copy.deepcopy(self.report)
        report["runs"][0]["degraded"] = True
        with self.assertRaisesRegex(ValueError, "degraded"):
            self.validate(report)

    def test_wrong_baseline(self):
        baseline = copy.deepcopy(self.baseline)
        baseline["policies"]["strict"]["metrics"]["ndcg10"] = 1.0
        with self.assertRaisesRegex(ValueError, "diagnostic baseline"):
            self.validate(self.report, baseline)

    def test_empty_query_result_still_counts(self):
        with self.assertRaises(ValueError):
            self.validate(self.mutated_ranking(lambda ranking: ranking.clear()))

    def test_duplicate_query(self):
        report = copy.deepcopy(self.report)
        rows = report["runs"][0]["queries"]
        rows[-1] = rows[0]
        with self.assertRaisesRegex(ValueError, "missing/duplicate query"):
            self.validate(report)

    def test_hand_computed_metrics(self):
        self.assertEqual(gate.metrics([], ["a"])["ndcg10"], 0.0)
        self.assertEqual(gate.metrics(["a"], [])["mrr10"], 0.0)
        self.assertEqual(gate.metrics(["a", "a", "b"], ["a", "b"])["ndcg10"], 1.0)
        self.assertEqual(gate.metrics(["x", "a"], ["a"])["mrr10"], 0.5)
        self.assertEqual(gate.metrics(list("0123456789a"), ["a"])["mrr10"], 0.0)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    GateTests.report = json.loads(args.report.read_text())
    unittest.main(argv=[__file__], verbosity=2)
