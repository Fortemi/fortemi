use matric_db::{PgLinkRepository, SnnSafetyPolicy, SnnStatus};
use sqlx::{postgres::PgPoolOptions, Executor, Row};

async fn isolated_pool() -> Option<sqlx::PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .ok()?;
    pool.execute(
        "CREATE TEMP TABLE link (
            id uuid PRIMARY KEY,
            from_note_id uuid NOT NULL,
            to_note_id uuid,
            kind text NOT NULL,
            score real NOT NULL,
            metadata jsonb NOT NULL DEFAULT '{}'::jsonb
        )",
    )
    .await
    .ok()?;
    Some(pool)
}

async fn link_fingerprint(pool: &sqlx::PgPool) -> (i64, String) {
    let row = sqlx::query(
        "SELECT COUNT(*) AS count,
                COALESCE(md5(string_agg(
                    id::text || '|' || score::text || '|' || metadata::text,
                    ',' ORDER BY id
                )), md5('')) AS checksum
           FROM link",
    )
    .fetch_one(pool)
    .await
    .expect("fingerprint links");
    (row.get("count"), row.get("checksum"))
}

async fn run_snn(
    pool: &sqlx::PgPool,
    dry_run: bool,
    policy: SnnSafetyPolicy,
) -> matric_db::SnnResult {
    let repository = PgLinkRepository::new(pool.clone());
    let mut transaction = pool.begin().await.expect("begin SNN transaction");
    let result = repository
        .recompute_snn_scores_tx(&mut transaction, 11, 0.10, dry_run, policy)
        .await
        .expect("compute SNN plan");
    transaction.commit().await.expect("commit SNN transaction");
    result
}

async fn insert_report_scale_triangle_free_graph(pool: &sqlx::PgPool) {
    sqlx::query(
        "WITH edge_plan AS (
            SELECT i,
                   ((i - 1) % 791 + 1)::int AS left_node,
                   ((i - 1) / 791)::int AS layer
              FROM generate_series(1, 11449) AS i
        )
        INSERT INTO link (id, from_note_id, to_note_id, kind, score, metadata)
        SELECT md5(('edge-' || i)::text)::uuid,
               md5(left_node::text)::uuid,
               md5((792 + ((left_node * 17 + layer) % 792))::text)::uuid,
               'semantic',
               0.9,
               jsonb_build_object('fixture_edge', i)
          FROM edge_plan",
    )
    .execute(pool)
    .await
    .expect("insert report-scale SNN fixture");
}

#[tokio::test]
async fn reported_scale_catastrophic_plan_is_identical_in_dry_run_and_commit_and_preserves_rows() {
    let Some(pool) = isolated_pool().await else {
        eprintln!("skipping PostgreSQL SNN test: DATABASE_URL unavailable");
        return;
    };
    insert_report_scale_triangle_free_graph(&pool).await;
    let before = link_fingerprint(&pool).await;
    assert_eq!(before.0, 11_449);

    let dry_run = run_snn(&pool, true, SnnSafetyPolicy::default()).await;
    let commit = run_snn(&pool, false, SnnSafetyPolicy::default()).await;

    for result in [&dry_run, &commit] {
        assert_eq!(result.status, SnnStatus::SafetyAborted);
        assert_eq!(result.total_edges, 11_449);
        assert_eq!(result.retained, 0);
        assert_eq!(result.pruned, 11_449);
        assert_eq!(result.k_used, 11);
        assert_eq!(result.threshold_used, 0.10);
        assert_eq!(result.snn_score_distribution[0], 11_449);
        assert!(result
            .safety_reasons
            .iter()
            .any(|reason| reason == "retention_ratio_below_minimum"));
        assert!(result.remediation.is_some());
    }
    assert_eq!(dry_run.retention_ratio, commit.retention_ratio);
    assert_eq!(dry_run.retained_mean_degree, commit.retained_mean_degree);
    assert_eq!(link_fingerprint(&pool).await, before);

    let override_result = run_snn(
        &pool,
        false,
        SnnSafetyPolicy {
            allow_aggressive_pruning: true,
            ..SnnSafetyPolicy::default()
        },
    )
    .await;
    assert_eq!(override_result.status, SnnStatus::Completed);
    assert!(override_result.aggressive_pruning_override);
    assert_eq!(link_fingerprint(&pool).await.0, 0);
}

#[tokio::test]
async fn small_already_sparse_disconnected_graph_is_skipped_without_mutation() {
    let Some(pool) = isolated_pool().await else {
        eprintln!("skipping PostgreSQL SNN test: DATABASE_URL unavailable");
        return;
    };
    sqlx::query(
        "INSERT INTO link (id, from_note_id, to_note_id, kind, score)
         SELECT md5(('edge-' || i)::text)::uuid,
                md5(('left-' || i)::text)::uuid,
                md5(('right-' || i)::text)::uuid,
                'semantic',
                0.9
           FROM generate_series(1, 6) AS i",
    )
    .execute(&pool)
    .await
    .expect("insert sparse disconnected fixture");
    let before = link_fingerprint(&pool).await;

    let result = run_snn(&pool, false, SnnSafetyPolicy::default()).await;
    assert_eq!(result.status, SnnStatus::SkippedSparse);
    assert_eq!(result.retained, 6);
    assert_eq!(result.pruned, 0);
    assert_eq!(link_fingerprint(&pool).await, before);
}

#[tokio::test]
async fn disconnected_multi_domain_dense_graph_is_safety_aborted_without_mutation() {
    let Some(pool) = isolated_pool().await else {
        eprintln!("skipping PostgreSQL SNN test: DATABASE_URL unavailable");
        return;
    };
    sqlx::query(
        "WITH domains AS (
             SELECT domain FROM generate_series(1, 2) AS domain
         ), edge_plan AS (
             SELECT domain, left_node, right_node
               FROM domains
               CROSS JOIN generate_series(1, 12) AS left_node
               CROSS JOIN generate_series(1, 12) AS right_node
         )
         INSERT INTO link (id, from_note_id, to_note_id, kind, score, metadata)
         SELECT md5((domain || '-edge-' || left_node || '-' || right_node)::text)::uuid,
                md5((domain || '-left-' || left_node)::text)::uuid,
                md5((domain || '-right-' || right_node)::text)::uuid,
                'semantic',
                0.9,
                jsonb_build_object('domain', domain)
           FROM edge_plan",
    )
    .execute(&pool)
    .await
    .expect("insert disconnected multi-domain fixture");
    let before = link_fingerprint(&pool).await;
    assert_eq!(before.0, 288);

    let result = run_snn(&pool, false, SnnSafetyPolicy::default()).await;
    assert_eq!(result.status, SnnStatus::SafetyAborted);
    assert_eq!(result.node_count, 48);
    assert_eq!(result.retained, 0);
    assert_eq!(result.pruned, 288);
    assert_eq!(link_fingerprint(&pool).await, before);
}
