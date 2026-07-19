use std::str::FromStr;
use std::sync::Arc;

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use matric_core::{
    MeteringError, UsageAttributeKey, UsageAttributeValue, UsageAttributes, UsageClass,
    UsageCorrelation, UsageDimension, UsageEvent, UsageMeasurement, UsageMeter, UsageOutcome,
    UsageProducer, UsageQuantity, UsageSource, UsageSubject,
};
use tracing::warn;
use uuid::Uuid;

use crate::handler::JobContext;

pub(crate) struct MediaUsageContext {
    meter: Arc<dyn UsageMeter>,
    subject: UsageSubject,
    job_id: Uuid,
    attempt: i32,
    event_time: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
}

impl MediaUsageContext {
    pub(crate) fn new(
        meter: Arc<dyn UsageMeter>,
        ctx: &JobContext,
        archive: &str,
    ) -> Result<Self, MeteringError> {
        Ok(Self {
            meter,
            subject: UsageSubject::unknown().with_archive(archive)?,
            job_id: ctx.job.id,
            attempt: ctx.job.retry_count,
            event_time: ctx.job.started_at.unwrap_or(ctx.job.created_at),
            recorded_at: Utc::now(),
        })
    }

    pub(crate) async fn record_audio_seconds(
        &self,
        duration_seconds: Option<f64>,
        outcome: UsageOutcome,
    ) {
        match media_usage_event(self, duration_seconds, outcome) {
            Ok(event) => {
                if let Err(error) = self.meter.record(&event).await {
                    warn!(
                        error_len = error.to_string().chars().count(),
                        "Best-effort media usage recording failed"
                    );
                }
            }
            Err(error) => {
                warn!(
                    error_len = error.to_string().chars().count(),
                    "Media usage event construction failed"
                );
            }
        }
    }
}

fn media_usage_event(
    context: &MediaUsageContext,
    duration_seconds: Option<f64>,
    outcome: UsageOutcome,
) -> Result<UsageEvent, MeteringError> {
    let dimension = UsageDimension::MediaProcessedSeconds;
    let (measurement, source) =
        match duration_seconds.filter(|value| value.is_finite() && *value >= 0.0) {
            Some(value) => {
                let decimal = BigDecimal::from_str(&value.to_string())
                    .map_err(|_| MeteringError::InvalidQuantity)?;
                (
                    UsageMeasurement::Measured(UsageQuantity::new(decimal, dimension.unit())?),
                    UsageSource::ProviderReported,
                )
            }
            None => (
                UsageMeasurement::Unavailable {
                    unit: dimension.unit(),
                },
                UsageSource::Unavailable,
            ),
        };
    let identity_name = format!("attempt:{}:media:audio_seconds", context.attempt);
    let event_id = Uuid::new_v5(&context.job_id, identity_name.as_bytes());
    let mut attrs = UsageAttributes::default();
    attrs.insert(
        &dimension,
        UsageAttributeKey::MediaKind,
        UsageAttributeValue::label("audio")?,
    )?;

    UsageEvent::new(
        format!(
            "job:{}:attempt:{}:media:audio_seconds:actual",
            context.job_id, context.attempt
        ),
        context.event_time,
        context.subject.clone(),
        dimension,
        measurement,
        UsageClass::BillableActual,
        UsageProducer::Jobs,
        source,
        outcome,
    )?
    .with_identity(event_id, context.recorded_at)
    .with_correlation(UsageCorrelation::default().with_job_id(context.job_id))?
    .with_attrs(attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn provider_duration_is_exact_private_and_replay_safe() {
        let meter = matric_core::InMemoryMeter::default();
        let event_time = Utc::now();
        let job_id = Uuid::now_v7();
        let context = MediaUsageContext {
            meter: Arc::new(meter.clone()),
            subject: UsageSubject::unknown()
                .with_archive("resolved-archive")
                .unwrap(),
            job_id,
            attempt: 1,
            event_time,
            recorded_at: event_time,
        };

        context
            .record_audio_seconds(Some(12.375), UsageOutcome::Completed)
            .await;
        context
            .record_audio_seconds(Some(12.375), UsageOutcome::Completed)
            .await;
        let events = meter.events().await;

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.dimension, UsageDimension::MediaProcessedSeconds);
        assert_eq!(
            event.measurement.quantity().unwrap().value().to_string(),
            "12.375"
        );
        assert_eq!(event.source, UsageSource::ProviderReported);
        assert_eq!(event.outcome, UsageOutcome::Completed);
        assert_eq!(event.subject.archive_id(), Some("resolved-archive"));
        assert_eq!(event.correlation.job_id(), Some(job_id));
        assert_eq!(event.attrs.len(), 1);

        let encoded = serde_json::to_string(event).unwrap();
        for forbidden in [
            "private transcript",
            "secret.wav",
            "https://user:pass@whisper.internal",
            "sk-secret",
        ] {
            assert!(!encoded.contains(forbidden));
        }
        assert!(!format!("{event:?}").contains("resolved-archive"));
    }

    #[test]
    fn missing_or_invalid_provider_duration_is_unavailable() {
        let event_time = Utc::now();
        let context = MediaUsageContext {
            meter: Arc::new(matric_core::NoOpMeter),
            subject: UsageSubject::unknown(),
            job_id: Uuid::now_v7(),
            attempt: 0,
            event_time,
            recorded_at: event_time,
        };

        for duration in [None, Some(f64::NAN), Some(-1.0), Some(f64::INFINITY)] {
            let event =
                media_usage_event(&context, duration, UsageOutcome::FailedAfterPartialUsage)
                    .unwrap();
            assert!(event.measurement.quantity().is_none());
            assert_eq!(event.source, UsageSource::Unavailable);
        }
    }

    #[test]
    fn retry_attempt_has_distinct_stable_identity() {
        let event_time = Utc::now();
        let first = MediaUsageContext {
            meter: Arc::new(matric_core::NoOpMeter),
            subject: UsageSubject::unknown(),
            job_id: Uuid::now_v7(),
            attempt: 0,
            event_time,
            recorded_at: event_time,
        };
        let retry = MediaUsageContext {
            attempt: 1,
            meter: first.meter.clone(),
            subject: first.subject.clone(),
            job_id: first.job_id,
            event_time: first.event_time,
            recorded_at: first.recorded_at,
        };

        let first_event = media_usage_event(&first, Some(2.0), UsageOutcome::Completed).unwrap();
        let replay = media_usage_event(&first, Some(2.0), UsageOutcome::Completed).unwrap();
        let retry_event = media_usage_event(&retry, Some(2.0), UsageOutcome::Completed).unwrap();

        assert_eq!(first_event, replay);
        assert_ne!(first_event.event_id, retry_event.event_id);
        assert_ne!(first_event.idempotency_key, retry_event.idempotency_key);
    }
}
