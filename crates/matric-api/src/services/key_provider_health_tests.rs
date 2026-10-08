//! State-machine tests for the cached key-provider health signal.

use super::*;

const INTERVAL: Duration = Duration::from_secs(60);

fn health(start: Instant) -> KeyProviderHealth {
    KeyProviderHealth::new(Some(INTERVAL), start)
}

#[test]
fn classification_follows_the_provider_taxonomy() {
    use KeyFailureClass as C;
    for class in [C::Throttled, C::ProviderUnavailable, C::ProviderFailure] {
        assert_eq!(health_effect(class), HealthEffect::Degrade, "{class:?}");
    }
    for class in [
        C::KeyDisabled,
        C::AccessDenied,
        C::KeyVersionUnavailable,
        C::InvalidConfiguration,
    ] {
        assert_eq!(health_effect(class), HealthEffect::Unavailable, "{class:?}");
    }
    for class in [
        C::InvalidContext,
        C::ContextMismatch,
        C::InvalidCiphertext,
        C::UnsupportedVersion,
        C::UnsupportedOperation,
    ] {
        assert_eq!(health_effect(class), HealthEffect::Ignore, "{class:?}");
    }
}

#[test]
fn throttling_degrades_but_stays_ready_then_recovers_on_success() {
    let start = Instant::now();
    let health = health(start);
    assert_eq!(health.snapshot(start), KeyHealthSnapshot::Ready);

    health.record_failure(KeyFailureClass::Throttled, start);
    let snapshot = health.snapshot(start);
    assert_eq!(
        snapshot,
        KeyHealthSnapshot::Degraded(KeyFailureClass::Throttled)
    );
    assert!(snapshot.permits_readiness());

    health.record_success(start + Duration::from_secs(1));
    assert_eq!(health.snapshot(start), KeyHealthSnapshot::Ready);
}

#[test]
fn terminal_key_failures_fail_readiness_until_a_later_success() {
    let start = Instant::now();
    let health = health(start);
    health.record_failure(KeyFailureClass::KeyDisabled, start);
    let snapshot = health.snapshot(start + Duration::from_secs(3600));
    assert_eq!(
        snapshot,
        KeyHealthSnapshot::Unavailable(KeyFailureClass::KeyDisabled)
    );
    assert!(!snapshot.permits_readiness());

    // A transient error must not mask the known terminal problem.
    health.record_failure(KeyFailureClass::Throttled, start);
    assert!(!health.snapshot(start).permits_readiness());

    health.record_success(start + Duration::from_secs(5));
    assert_eq!(health.snapshot(start), KeyHealthSnapshot::Ready);
}

#[test]
fn per_request_input_errors_do_not_change_health() {
    let start = Instant::now();
    let health = health(start);
    health.record_failure(KeyFailureClass::ContextMismatch, start);
    health.record_failure(KeyFailureClass::InvalidCiphertext, start);
    assert_eq!(health.snapshot(start), KeyHealthSnapshot::Ready);
}

#[test]
fn degraded_signal_is_bounded_by_its_ttl() {
    let start = Instant::now();
    let health = health(start);
    health.record_failure(KeyFailureClass::ProviderUnavailable, start);
    assert!(matches!(
        health.snapshot(start + INTERVAL),
        KeyHealthSnapshot::Degraded(_)
    ));
    assert_eq!(
        health.snapshot(start + INTERVAL * 2),
        KeyHealthSnapshot::Ready
    );
}

#[test]
fn canary_runs_at_most_once_per_interval_and_only_without_recent_success() {
    let start = Instant::now();
    let health = health(start);
    assert!(!health.canary_due(start + Duration::from_secs(1)));
    assert!(health.canary_due(start + INTERVAL));

    // Real traffic proves health: no canary needed.
    health.record_success(start + INTERVAL);
    assert!(!health.canary_due(start + INTERVAL + Duration::from_secs(1)));

    // An unhealthy provider is re-probed once per interval.
    health.record_failure(KeyFailureClass::AccessDenied, start + INTERVAL);
    assert!(health.canary_due(start + INTERVAL));
    health.mark_canary(start + INTERVAL);
    assert!(!health.canary_due(start + INTERVAL + Duration::from_secs(30)));
    assert!(health.canary_due(start + INTERVAL * 2));
}

#[test]
fn disabled_canary_is_never_due() {
    let start = Instant::now();
    let health = KeyProviderHealth::new(None, start);
    health.record_failure(KeyFailureClass::KeyDisabled, start);
    assert!(!health.canary_due(start + Duration::from_secs(86_400)));
}

#[test]
fn canary_interval_parsing_is_strict() {
    assert_eq!(
        parse_canary_interval(None),
        Ok(Some(DEFAULT_CANARY_INTERVAL))
    );
    assert_eq!(parse_canary_interval(Some("0")), Ok(None));
    assert_eq!(
        parse_canary_interval(Some("300")),
        Ok(Some(Duration::from_secs(300)))
    );
    for invalid in ["5", "-1", "1m", ""] {
        assert!(parse_canary_interval(Some(invalid)).is_err(), "{invalid}");
    }
}

#[test]
fn readiness_body_is_backward_compatible_and_reports_key_state() {
    let (status, body) = readiness_response(true, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ready" }));

    let (status, body) = readiness_response(false, None);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["reason_code"], "required_dependency_unavailable");

    let degraded = KeyHealthSnapshot::Degraded(KeyFailureClass::Throttled);
    let (status, body) = readiness_response(true, Some(degraded));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "status": "ready",
            "key_provider": { "status": "degraded", "reason_code": "throttled" }
        })
    );

    let unavailable = KeyHealthSnapshot::Unavailable(KeyFailureClass::KeyDisabled);
    let (status, body) = readiness_response(true, Some(unavailable));
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["reason_code"], "required_dependency_unavailable");
    assert_eq!(
        body["key_provider"],
        json!({ "status": "unavailable", "reason_code": "key_disabled" })
    );
}
