use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Deserialize)]
pub struct Fixture {
    pub notes: Vec<Note>,
    pub queries: Vec<Query>,
}
#[derive(Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}
#[derive(Deserialize)]
pub struct Query {
    pub id: String,
    pub text: String,
    pub relevant_notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Metrics {
    pub ndcg10: f64,
    pub mrr10: f64,
    pub recall5: f64,
    pub recall10: f64,
    pub recall100: f64,
}

/// Binary qrels, gain=1, log2(rank+1), unjudged=0, duplicates removed
/// before cutoffs, and zero for empty rankings or no relevant judgments.
pub fn evaluate(ranking: &[String], relevant: &[String]) -> Metrics {
    let relevant: HashSet<_> = relevant.iter().collect();
    if relevant.is_empty() {
        return Metrics::default();
    }
    let mut seen = HashSet::new();
    let ranking: Vec<_> = ranking.iter().filter(|id| seen.insert(*id)).collect();
    let recall = |k| {
        ranking
            .iter()
            .take(k)
            .filter(|id| relevant.contains(**id))
            .count() as f64
            / relevant.len() as f64
    };
    let dcg: f64 = ranking
        .iter()
        .take(10)
        .enumerate()
        .filter(|(_, id)| relevant.contains(**id))
        .map(|(i, _)| 1.0 / ((i + 2) as f64).log2())
        .sum();
    let ideal: f64 = (0..relevant.len().min(10))
        .map(|i| 1.0 / ((i + 2) as f64).log2())
        .sum();
    Metrics {
        ndcg10: dcg / ideal,
        mrr10: ranking
            .iter()
            .take(10)
            .position(|id| relevant.contains(*id))
            .map(|i| 1.0 / (i + 1) as f64)
            .unwrap_or(0.0),
        recall5: recall(5),
        recall10: recall(10),
        recall100: recall(100),
    }
}

pub fn mean(metrics: &[Metrics]) -> Metrics {
    assert!(!metrics.is_empty());
    let n = metrics.len() as f64;
    Metrics {
        ndcg10: metrics.iter().map(|m| m.ndcg10).sum::<f64>() / n,
        mrr10: metrics.iter().map(|m| m.mrr10).sum::<f64>() / n,
        recall5: metrics.iter().map(|m| m.recall5).sum::<f64>() / n,
        recall10: metrics.iter().map(|m| m.recall10).sum::<f64>() / n,
        recall100: metrics.iter().map(|m| m.recall100).sum::<f64>() / n,
    }
}

#[test]
fn metric_examples() {
    let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let qrels = strings(&["a", "b"]);
    assert_eq!(evaluate(&qrels, &qrels).ndcg10, 1.0);
    assert_eq!(evaluate(&[], &qrels), Metrics::default());
    assert_eq!(evaluate(&qrels, &[]), Metrics::default());
    let second = evaluate(&strings(&["x", "a", "a"]), &qrels);
    assert!((second.ndcg10 - (1.0 / 3_f64.log2()) / (1.0 + 1.0 / 3_f64.log2())).abs() < 1e-12);
    assert_eq!(second.mrr10, 0.5);
    assert_eq!(second.recall5, 0.5);
    let outside = strings(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a"]);
    assert_eq!(evaluate(&outside, &qrels).mrr10, 0.0);
    assert_eq!(evaluate(&outside, &qrels).recall100, 0.5);
    assert_eq!(
        mean(&[Metrics::default(), evaluate(&qrels, &qrels)]).mrr10,
        0.5
    );
}
