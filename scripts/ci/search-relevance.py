#!/usr/bin/env python3
"""Validate the frozen binary keyword relevance contract; stdlib only."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess

METRICS = ("ndcg10", "mrr10", "recall5", "recall10", "recall100")
EPSILON = 0.000001


def metrics(ranking, qrels):
    ranking = list(dict.fromkeys(ranking))
    relevant = set(qrels)
    if not relevant:
        return dict.fromkeys(METRICS, 0.0)
    dcg = sum(1 / math.log2(i + 2) for i, note in enumerate(ranking[:10]) if note in relevant)
    ideal = sum(1 / math.log2(i + 2) for i in range(min(10, len(relevant))))
    result = {"ndcg10": dcg / ideal, "mrr10": next((1 / (i + 1) for i, note in enumerate(ranking[:10]) if note in relevant), 0.0)}
    for k in (5, 10, 100):
        result[f"recall{k}"] = len(set(ranking[:k]) & relevant) / len(relevant)
    return result


def require(condition, message):
    if not condition:
        raise ValueError(message)


def close(actual, expected, label):
    require(isinstance(actual, (int, float)) and math.isfinite(actual), f"{label}: nonfinite/non-numeric value")
    require(abs(actual - expected) <= EPSILON, f"{label}: reported {actual}, recomputed {expected}")


def validate_baseline(baseline, query_ids):
    # A NaN or out-of-range floor would make every comparison false and pass silently.
    for mode in ("strict", "or", "adaptive"):
        policy = baseline["policies"][mode]
        require(set(policy["queries"]) == set(query_ids), f"baseline {mode}: query coverage changed")
        for label, values in [(f"{mode}", policy["metrics"]),
                              *((f"{mode}/{qid}", policy["queries"][qid]) for qid in sorted(query_ids))]:
            for metric in METRICS:
                value = values[metric]
                require(type(value) in (int, float) and math.isfinite(value) and 0.0 <= value <= 1.0,
                        f"baseline {label}/{metric}: must be a finite number in [0, 1]")


def validate(report, fixture, baseline):
    require(report["evaluator"] == baseline["evaluator"] == "binary-v1", "wrong evaluator")
    require(report["route"] == "SearchRequest::execute_on_connection", "wrong measurement route")
    require(report["repetitions"] == 10, "ten deterministic repetitions required")
    require(report["configuration"]["adaptive_policy"] == baseline["adaptive_policy"], "adaptive defaults changed: reviewed baseline update required")
    require(baseline["epsilon"] == EPSILON, "baseline tolerance changed")
    queries = {q["id"]: q for q in fixture["queries"]}
    notes = {n["id"] for n in fixture["notes"]}
    require(len(queries) == len(fixture["queries"]) == 8, "query coverage changed")
    require(len(notes) == len(fixture["notes"]) == 15, "note coverage changed")
    validate_baseline(baseline, queries)
    modes = [run["mode"] for run in report["runs"]]
    require(sorted(modes) == ["adaptive", "or", "strict"], "missing, duplicated, or mislabeled policy")
    failures = []
    for run in report["runs"]:
        mode = run["mode"]
        require(run["requested_mode"] == run["effective_mode"] == "fts" and run["degraded"] is False,
                f"{mode}: keyword results cannot masquerade as semantic/hybrid or degraded results")
        rows = run["queries"]
        require(len(rows) == len(queries) and {r["id"] for r in rows} == set(queries), f"{mode}: missing/duplicate query")
        measured = []
        for row in rows:
            query = queries[row["id"]]
            label = f"{mode}/{row['id']}"
            require(row["text"] == query["text"], f"{label}: query text changed")
            ranking = row["ranking"]
            require(len(set(ranking)) == len(ranking), f"{label}: duplicate notes")
            require(set(ranking) <= notes, f"{label}: unknown note")
            result = metrics(ranking, query["relevant_notes"])
            measured.append(result)
            for metric in METRICS:
                close(row["metrics"][metric], result[metric], f"{label}/{metric}")
                floor = baseline["policies"][mode]["queries"][row["id"]][metric]
                if result[metric] + EPSILON < floor:
                    failures.append(f"{label}/{metric}: {result[metric]:.9f} < {floor:.9f}")
        for metric in METRICS:
            value = sum(m[metric] for m in measured) / len(queries)
            close(run["metrics"][metric], value, f"{mode}/{metric}")
            expected = baseline["policies"][mode]["metrics"][metric]
            if value + EPSILON < expected:
                failures.append(f"{mode}/{metric}: {value:.9f} < {expected:.9f}")
            # Strict/OR are frozen diagnostics, not silently replaceable baselines.
            if mode != "adaptive":
                close(value, expected, f"{mode}/{metric} diagnostic baseline")
        if mode == "adaptive":
            require(run["metrics"]["ndcg10"] >= 0.90 and run["metrics"]["mrr10"] >= 0.90,
                    "adaptive release floors not met")
        close(run["zero_result_rate"], sum(not r["ranking"] for r in rows) / len(queries), f"{mode}/zero-result rate")
        require(math.isfinite(run["p95_ms"]) and run["p95_ms"] >= 0, f"{mode}: invalid latency")
    require(not failures, "relevance regressions:\n" + "\n".join(failures))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, default=Path("crates/matric-search/tests/fixtures/golden_test_set.json"))
    parser.add_argument("--baseline", type=Path, default=Path("crates/matric-search/tests/fixtures/keyword_relevance_baseline.json"))
    args = parser.parse_args()
    fixture_bytes = args.fixture.read_bytes()
    fixture = json.loads(fixture_bytes)
    baseline = json.loads(args.baseline.read_text())
    report = json.loads(args.report.read_text())
    digest = hashlib.sha256(fixture_bytes).hexdigest()
    require(digest == baseline["fixture_sha256"], "fixture/qrels changed: reviewed baseline update required")
    revision = os.environ.get("FORTEMI_RELEVANCE_SOURCE_REVISION") or subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    require(re.fullmatch(r"[0-9a-f]{40}", revision), "source revision must be a full commit SHA")
    report.update(fixture_sha256=digest, source_revision=revision,
                  baseline_sha256=hashlib.sha256(args.baseline.read_bytes()).hexdigest(),
                  embedding_model=None, semantic_hybrid_relevance="unmeasured",
                  epsilon=EPSILON)
    # Preserve provenance and failing per-query results even if validation fails.
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    validate(report, fixture, baseline)
    for run in report["runs"]:
        print(f"{run['mode']}: " + ", ".join(f"{k}={run['metrics'][k]:.6f}" for k in METRICS))
    print("Keyword relevance gate passed (all eight queries, ten repetitions per policy).")


if __name__ == "__main__":
    main()
