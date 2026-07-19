//! Usage metering and quota policy contracts.
//!
//! Metering records accounting facts; quota policy controls admission. They
//! deliberately remain separate from security audit events and from any one
//! billing provider. CE defaults are explicit no-ops. Hosted hard-cap
//! implementations must treat unavailable reservation state as fail-closed.
//!
//! ```
//! use chrono::Utc;
//! use matric_core::{
//!     UsageClass, UsageDimension, UsageEvent, UsageMeasurement, UsageOutcome,
//!     UsageProducer, UsageQuantity, UsageSource, UsageSubject, UsageUnit,
//! };
//!
//! let event = UsageEvent::new(
//!     "request:018fd1a0:actual",
//!     Utc::now(),
//!     UsageSubject::unknown(),
//!     UsageDimension::ApiRequest,
//!     UsageMeasurement::Measured(UsageQuantity::whole(1, UsageUnit::Count)?),
//!     UsageClass::BillableActual,
//!     UsageProducer::Api,
//!     UsageSource::LocalMeasured,
//!     UsageOutcome::Completed,
//! )?;
//! assert_eq!(event.measurement.quantity().unwrap().value().to_string(), "1");
//! # Ok::<(), matric_core::MeteringError>(())
//! ```

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

const METERING_SCHEMA_VERSION: u16 = 1;
const MAX_ID_BYTES: usize = 256;
const MAX_LABEL_BYTES: usize = 128;

/// An exact usage quantity paired with its semantic unit.
///
/// `BigDecimal` is serialized as a string, avoiding binary floating-point drift
/// across ledger and billing-sink boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UsageQuantity {
    value: BigDecimal,
    unit: UsageUnit,
}

impl UsageQuantity {
    pub fn new(value: BigDecimal, unit: UsageUnit) -> Result<Self, MeteringError> {
        unit.validate()?;
        let quantity = Self {
            value: value.normalized(),
            unit,
        };
        quantity.validate()?;
        Ok(quantity)
    }

    pub fn whole(value: u64, unit: UsageUnit) -> Result<Self, MeteringError> {
        Self::new(BigDecimal::from(value), unit)
    }

    pub fn zero(unit: UsageUnit) -> Result<Self, MeteringError> {
        Self::new(BigDecimal::from(0), unit)
    }

    pub fn value(&self) -> &BigDecimal {
        &self.value
    }

    pub fn unit(&self) -> &UsageUnit {
        &self.unit
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, MeteringError> {
        if self.unit != other.unit {
            return Err(MeteringError::UnitMismatch);
        }
        let value = &self.value + &other.value;
        Self::new(value, self.unit.clone())
    }

    fn validate(&self) -> Result<(), MeteringError> {
        let (integer, scale) = self.value.as_bigint_and_exponent();
        let coefficient_digits = integer.to_string().trim_start_matches('-').len();
        let expanded_integer_digits = if scale < 0 {
            coefficient_digits.saturating_add(scale.unsigned_abs() as usize)
        } else {
            coefficient_digits
        };
        if scale > 18 || coefficient_digits > 38 || expanded_integer_digits > 38 {
            return Err(MeteringError::InvalidQuantity);
        }
        self.unit.validate()
    }

    fn is_negative(&self) -> bool {
        self.value < 0
    }

    fn is_positive(&self) -> bool {
        self.value > 0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageUnit {
    Count,
    Token,
    Byte,
    Millisecond,
    Second,
    Vector,
    CurrencyMinorUnit { currency: String },
    Custom(String),
}

impl UsageUnit {
    fn validate(&self) -> Result<(), MeteringError> {
        match self {
            Self::CurrencyMinorUnit { currency }
                if currency.len() != 3
                    || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) =>
            {
                return Err(MeteringError::InvalidUnit);
            }
            Self::Custom(name) => validate_label(name, "custom unit")?,
            _ => {}
        }
        Ok(())
    }
}

/// A measured quantity or an explicitly unavailable measurement.
///
/// Unavailable usage is never represented as numeric zero.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageMeasurement {
    Measured(UsageQuantity),
    Unavailable { unit: UsageUnit },
}

impl UsageMeasurement {
    pub fn unit(&self) -> &UsageUnit {
        match self {
            Self::Measured(quantity) => quantity.unit(),
            Self::Unavailable { unit } => unit,
        }
    }

    pub fn quantity(&self) -> Option<&UsageQuantity> {
        match self {
            Self::Measured(quantity) => Some(quantity),
            Self::Unavailable { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageDimension {
    ApiRequest,
    InferenceInputTokens,
    InferenceOutputTokens,
    CachedInputTokens,
    ReasoningOutputTokens,
    AudioInputTokens,
    AudioOutputTokens,
    EmbeddingTokens,
    EmbeddingVectors,
    StorageBytes,
    IngestRows,
    JobEnqueued,
    ActiveJob,
    ActiveStream,
    ConcurrentOperation,
    MediaProcessedSeconds,
    RealtimeAudioSeconds,
    BridgeSession,
    BridgeProviderCall,
    McpToolCall,
    SaturationSignal,
    Custom { name: String, unit: UsageUnit },
}

impl UsageDimension {
    pub fn unit(&self) -> UsageUnit {
        match self {
            Self::InferenceInputTokens
            | Self::InferenceOutputTokens
            | Self::CachedInputTokens
            | Self::ReasoningOutputTokens
            | Self::AudioInputTokens
            | Self::AudioOutputTokens
            | Self::EmbeddingTokens => UsageUnit::Token,
            Self::EmbeddingVectors => UsageUnit::Vector,
            Self::StorageBytes => UsageUnit::Byte,
            Self::MediaProcessedSeconds | Self::RealtimeAudioSeconds => UsageUnit::Second,
            Self::Custom { unit, .. } => unit.clone(),
            _ => UsageUnit::Count,
        }
    }

    fn validate(&self) -> Result<(), MeteringError> {
        if let Self::Custom { name, unit } = self {
            validate_label(name, "custom dimension")?;
            unit.validate()?;
        }
        Ok(())
    }

    fn allows_attribute(&self, key: UsageAttributeKey) -> bool {
        use UsageAttributeKey as Key;

        match key {
            Key::PricingVersion | Key::Currency => !matches!(self, Self::SaturationSignal),
            Key::RouteClass => matches!(self, Self::ApiRequest),
            Key::ToolClass => matches!(self, Self::McpToolCall),
            Key::JobKind | Key::ResourceClass => {
                matches!(self, Self::JobEnqueued | Self::ActiveJob)
            }
            Key::MediaKind => matches!(
                self,
                Self::MediaProcessedSeconds
                    | Self::RealtimeAudioSeconds
                    | Self::AudioInputTokens
                    | Self::AudioOutputTokens
            ),
            Key::Provider
            | Key::Model
            | Key::Endpoint
            | Key::Protocol
            | Key::CacheState
            | Key::EstimatorVersion => matches!(
                self,
                Self::InferenceInputTokens
                    | Self::InferenceOutputTokens
                    | Self::CachedInputTokens
                    | Self::ReasoningOutputTokens
                    | Self::AudioInputTokens
                    | Self::AudioOutputTokens
                    | Self::EmbeddingTokens
                    | Self::EmbeddingVectors
                    | Self::MediaProcessedSeconds
                    | Self::RealtimeAudioSeconds
                    | Self::BridgeSession
                    | Self::BridgeProviderCall
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAttributeKey {
    Provider,
    Model,
    Endpoint,
    Protocol,
    RouteClass,
    ToolClass,
    JobKind,
    MediaKind,
    ResourceClass,
    CacheState,
    EstimatorVersion,
    PricingVersion,
    Currency,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAttributeValue {
    Label(String),
    Boolean(bool),
    Integer(i64),
}

impl UsageAttributeValue {
    pub fn label(value: impl Into<String>) -> Result<Self, MeteringError> {
        let value = value.into();
        validate_label(&value, "attribute label")?;
        Ok(Self::Label(value))
    }

    fn validate(&self) -> Result<(), MeteringError> {
        if let Self::Label(value) = self {
            validate_label(value, "attribute label")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsageAttributes(BTreeMap<UsageAttributeKey, UsageAttributeValue>);

impl UsageAttributes {
    pub fn insert(
        &mut self,
        dimension: &UsageDimension,
        key: UsageAttributeKey,
        value: UsageAttributeValue,
    ) -> Result<Option<UsageAttributeValue>, MeteringError> {
        if !dimension.allows_attribute(key) {
            return Err(MeteringError::AttributeNotAllowed(key));
        }
        value.validate()?;
        Ok(self.0.insert(key, value))
    }

    pub fn get(&self, key: UsageAttributeKey) -> Option<&UsageAttributeValue> {
        self.0.get(&key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn validate_for(&self, dimension: &UsageDimension) -> Result<(), MeteringError> {
        for (key, value) in &self.0 {
            if !dimension.allows_attribute(*key) {
                return Err(MeteringError::AttributeNotAllowed(*key));
            }
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct UsageSubject {
    tenant_id: Option<String>,
    principal_id: Option<String>,
    client_id: Option<String>,
    archive_id: Option<String>,
    anonymous_key: Option<String>,
}

impl fmt::Debug for UsageSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UsageSubject")
            .field("tenant_id_present", &self.tenant_id.is_some())
            .field("principal_id_present", &self.principal_id.is_some())
            .field("client_id_present", &self.client_id.is_some())
            .field("archive_id_present", &self.archive_id.is_some())
            .field("anonymous_key_present", &self.anonymous_key.is_some())
            .finish()
    }
}

impl UsageSubject {
    /// Construct an unattributed subject.
    ///
    /// Hosted quota enforcement must apply an explicit policy to this subject
    /// rather than infer identity from untrusted request fields.
    pub fn unknown() -> Self {
        Self {
            tenant_id: None,
            principal_id: None,
            client_id: None,
            archive_id: None,
            anonymous_key: None,
        }
    }

    pub fn anonymous(key: impl Into<String>) -> Result<Self, MeteringError> {
        Self::unknown().with_anonymous_key(key)
    }

    pub fn with_tenant(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.tenant_id = Some(validate_id(id.into(), "tenant id")?);
        Ok(self)
    }

    pub fn with_principal(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.principal_id = Some(validate_id(id.into(), "principal id")?);
        Ok(self)
    }

    pub fn with_client(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.client_id = Some(validate_id(id.into(), "client id")?);
        Ok(self)
    }

    pub fn with_archive(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.archive_id = Some(validate_id(id.into(), "archive id")?);
        Ok(self)
    }

    pub fn with_anonymous_key(mut self, key: impl Into<String>) -> Result<Self, MeteringError> {
        self.anonymous_key = Some(validate_id(key.into(), "anonymous key")?);
        Ok(self)
    }

    pub fn is_unknown(&self) -> bool {
        self.tenant_id.is_none()
            && self.principal_id.is_none()
            && self.client_id.is_none()
            && self.archive_id.is_none()
            && self.anonymous_key.is_none()
    }

    pub fn is_anonymous(&self) -> bool {
        self.anonymous_key.is_some()
    }

    pub fn tenant_id(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }

    pub fn principal_id(&self) -> Option<&str> {
        self.principal_id.as_deref()
    }

    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    pub fn archive_id(&self) -> Option<&str> {
        self.archive_id.as_deref()
    }

    pub fn anonymous_key(&self) -> Option<&str> {
        self.anonymous_key.as_deref()
    }

    fn validate(&self) -> Result<(), MeteringError> {
        for (value, field) in [
            (self.tenant_id.as_deref(), "tenant id"),
            (self.principal_id.as_deref(), "principal id"),
            (self.client_id.as_deref(), "client id"),
            (self.archive_id.as_deref(), "archive id"),
            (self.anonymous_key.as_deref(), "anonymous key"),
        ] {
            if let Some(value) = value {
                validate_id_ref(value, field)?;
            }
        }
        Ok(())
    }
}

impl Default for UsageSubject {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct UsageCorrelation {
    request_id: Option<String>,
    job_id: Option<Uuid>,
    bridge_session_id: Option<String>,
    mcp_call_id: Option<String>,
    provider_call_id: Option<String>,
}

impl fmt::Debug for UsageCorrelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UsageCorrelation")
            .field("request_id_present", &self.request_id.is_some())
            .field("job_id_present", &self.job_id.is_some())
            .field(
                "bridge_session_id_present",
                &self.bridge_session_id.is_some(),
            )
            .field("mcp_call_id_present", &self.mcp_call_id.is_some())
            .field("provider_call_id_present", &self.provider_call_id.is_some())
            .finish()
    }
}

impl UsageCorrelation {
    pub fn with_request_id(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.request_id = Some(validate_id(id.into(), "request id")?);
        Ok(self)
    }

    pub fn with_job_id(mut self, id: Uuid) -> Self {
        self.job_id = Some(id);
        self
    }

    pub fn with_bridge_session_id(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.bridge_session_id = Some(validate_id(id.into(), "bridge session id")?);
        Ok(self)
    }

    pub fn with_mcp_call_id(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.mcp_call_id = Some(validate_id(id.into(), "MCP call id")?);
        Ok(self)
    }

    pub fn with_provider_call_id(mut self, id: impl Into<String>) -> Result<Self, MeteringError> {
        self.provider_call_id = Some(validate_id(id.into(), "provider call id")?);
        Ok(self)
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn job_id(&self) -> Option<Uuid> {
        self.job_id
    }

    pub fn bridge_session_id(&self) -> Option<&str> {
        self.bridge_session_id.as_deref()
    }

    pub fn mcp_call_id(&self) -> Option<&str> {
        self.mcp_call_id.as_deref()
    }

    pub fn provider_call_id(&self) -> Option<&str> {
        self.provider_call_id.as_deref()
    }

    fn validate(&self) -> Result<(), MeteringError> {
        for (value, field) in [
            (self.request_id.as_deref(), "request id"),
            (self.bridge_session_id.as_deref(), "bridge session id"),
            (self.mcp_call_id.as_deref(), "MCP call id"),
            (self.provider_call_id.as_deref(), "provider call id"),
        ] {
            if let Some(value) = value {
                validate_id_ref(value, field)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageClass {
    BillableActual,
    NonBillableEstimate,
    NonBillableAdmission,
    NonBillableSaturation,
    Reversal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageProducer {
    Api,
    Jobs,
    Bridge,
    Mcp,
    Realtime,
    Inference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    ProviderReported,
    LocalMeasured,
    Estimated,
    Cache,
    Admission,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageOutcome {
    Completed,
    ClientInterrupted,
    ProviderInterrupted,
    Denied,
    FailedBeforeUsage,
    FailedAfterPartialUsage,
    Corrected,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct UsageEvent {
    pub schema_version: u16,
    pub event_id: Uuid,
    pub idempotency_key: String,
    pub event_time: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub subject: UsageSubject,
    pub dimension: UsageDimension,
    pub measurement: UsageMeasurement,
    pub class: UsageClass,
    pub producer: UsageProducer,
    pub source: UsageSource,
    pub outcome: UsageOutcome,
    pub correlation: UsageCorrelation,
    pub attrs: UsageAttributes,
}

impl fmt::Debug for UsageEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UsageEvent")
            .field("schema_version", &self.schema_version)
            .field("event_id_present", &true)
            .field("idempotency_key_present", &true)
            .field("event_time", &self.event_time)
            .field("recorded_at", &self.recorded_at)
            .field("subject", &self.subject)
            .field("dimension", &self.dimension)
            .field("measurement", &self.measurement)
            .field("class", &self.class)
            .field("producer", &self.producer)
            .field("source", &self.source)
            .field("outcome", &self.outcome)
            .field("correlation", &self.correlation)
            .field("attr_count", &self.attrs.len())
            .finish()
    }
}

impl UsageEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        idempotency_key: impl Into<String>,
        event_time: DateTime<Utc>,
        subject: UsageSubject,
        dimension: UsageDimension,
        measurement: UsageMeasurement,
        class: UsageClass,
        producer: UsageProducer,
        source: UsageSource,
        outcome: UsageOutcome,
    ) -> Result<Self, MeteringError> {
        let event = Self {
            schema_version: METERING_SCHEMA_VERSION,
            event_id: crate::new_v7(),
            idempotency_key: validate_id(idempotency_key.into(), "idempotency key")?,
            event_time,
            recorded_at: Utc::now(),
            subject,
            dimension,
            measurement,
            class,
            producer,
            source,
            outcome,
            correlation: UsageCorrelation::default(),
            attrs: UsageAttributes::default(),
        };
        event.validate()?;
        Ok(event)
    }

    pub fn with_identity(mut self, event_id: Uuid, recorded_at: DateTime<Utc>) -> Self {
        self.event_id = event_id;
        self.recorded_at = recorded_at;
        self
    }

    pub fn with_correlation(
        mut self,
        correlation: UsageCorrelation,
    ) -> Result<Self, MeteringError> {
        self.correlation = correlation;
        self.validate()?;
        Ok(self)
    }

    pub fn with_attrs(mut self, attrs: UsageAttributes) -> Result<Self, MeteringError> {
        self.attrs = attrs;
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), MeteringError> {
        if self.schema_version != METERING_SCHEMA_VERSION {
            return Err(MeteringError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.event_id.is_nil() {
            return Err(MeteringError::InvalidIdentifier("event id"));
        }
        validate_id_ref(&self.idempotency_key, "idempotency key")?;
        self.subject.validate()?;
        self.correlation.validate()?;
        self.dimension.validate()?;
        self.measurement.unit().validate()?;
        if let UsageMeasurement::Measured(quantity) = &self.measurement {
            quantity.validate()?;
        }
        if self.measurement.unit() != &self.dimension.unit() {
            return Err(MeteringError::UnitMismatch);
        }
        self.attrs.validate_for(&self.dimension)?;

        match (&self.measurement, self.source) {
            (UsageMeasurement::Unavailable { .. }, UsageSource::Unavailable) => {}
            (UsageMeasurement::Unavailable { .. }, _) | (_, UsageSource::Unavailable) => {
                return Err(MeteringError::InvalidMeasurementSource);
            }
            _ => {}
        }

        if let UsageMeasurement::Measured(quantity) = &self.measurement {
            if self.class == UsageClass::Reversal && quantity.is_positive() {
                return Err(MeteringError::InvalidReversal);
            }
            if self.class != UsageClass::Reversal && quantity.is_negative() {
                return Err(MeteringError::InvalidQuantity);
            }
        }
        Ok(())
    }

    fn affects_actual_total(&self) -> bool {
        matches!(
            self.class,
            UsageClass::BillableActual | UsageClass::Reversal
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UsageAggregate {
    pub quantity: UsageQuantity,
    pub unavailable_events: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeWindow {
    LastMinute,
    LastHour,
    LastDay,
    LastMonth,
    Lifetime,
    Range {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

impl TimeWindow {
    fn validate(&self) -> Result<(), MeteringError> {
        if let Self::Range { start, end } = self {
            if start >= end {
                return Err(MeteringError::InvalidTimeWindow);
            }
        }
        Ok(())
    }

    fn contains(&self, event_time: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        match self {
            Self::LastMinute => event_time >= now - chrono::Duration::minutes(1),
            Self::LastHour => event_time >= now - chrono::Duration::hours(1),
            Self::LastDay => event_time >= now - chrono::Duration::days(1),
            Self::LastMonth => event_time >= now - chrono::Duration::days(30),
            Self::Lifetime => true,
            Self::Range { start, end } => event_time >= *start && event_time < *end,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateIdentity {
    EventId,
    IdempotencyKey,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum MeteringError {
    #[error("invalid metering identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid metering label: {0}")]
    InvalidLabel(&'static str),
    #[error("invalid metering time window")]
    InvalidTimeWindow,
    #[error("invalid usage unit")]
    InvalidUnit,
    #[error("invalid usage quantity")]
    InvalidQuantity,
    #[error("usage unit does not match dimension")]
    UnitMismatch,
    #[error("usage attribute is not allowed for dimension: {0:?}")]
    AttributeNotAllowed(UsageAttributeKey),
    #[error("unavailable measurement and source are inconsistent")]
    InvalidMeasurementSource,
    #[error("reversal quantity must not be positive")]
    InvalidReversal,
    #[error("unsupported metering schema version: {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("conflicting duplicate usage event: {0:?}")]
    DuplicateConflict(DuplicateIdentity),
    #[error("metering backend unavailable")]
    BackendUnavailable,
}

#[async_trait]
pub trait UsageMeter: Send + Sync {
    /// Record one immutable event.
    ///
    /// Implementations must treat an exact replay as success and reject a
    /// conflicting event or idempotency identity.
    async fn record(&self, event: &UsageEvent) -> Result<(), MeteringError>;

    async fn current(
        &self,
        subject: &UsageSubject,
        dimension: &UsageDimension,
        window: TimeWindow,
    ) -> Result<UsageAggregate, MeteringError>;

    async fn flush(&self, grace: Duration) -> Result<(), MeteringError>;
}

#[derive(Clone, Debug, Default)]
pub struct NoOpMeter;

#[async_trait]
impl UsageMeter for NoOpMeter {
    async fn record(&self, event: &UsageEvent) -> Result<(), MeteringError> {
        event.validate()
    }

    async fn current(
        &self,
        subject: &UsageSubject,
        dimension: &UsageDimension,
        window: TimeWindow,
    ) -> Result<UsageAggregate, MeteringError> {
        subject.validate()?;
        dimension.validate()?;
        window.validate()?;
        Ok(UsageAggregate {
            quantity: UsageQuantity::zero(dimension.unit())?,
            unavailable_events: 0,
        })
    }

    async fn flush(&self, _: Duration) -> Result<(), MeteringError> {
        Ok(())
    }
}

#[derive(Default)]
struct InMemoryMeterState {
    events: HashMap<Uuid, UsageEvent>,
    idempotency: HashMap<String, Uuid>,
}

/// A deterministic, non-durable recorder for tests and local observability.
#[derive(Clone, Default)]
pub struct InMemoryMeter {
    state: Arc<Mutex<InMemoryMeterState>>,
}

impl InMemoryMeter {
    pub async fn events(&self) -> Vec<UsageEvent> {
        let state = self.state.lock().await;
        let mut events: Vec<_> = state.events.values().cloned().collect();
        events.sort_by_key(|event| (event.recorded_at, event.event_id));
        events
    }
}

#[async_trait]
impl UsageMeter for InMemoryMeter {
    async fn record(&self, event: &UsageEvent) -> Result<(), MeteringError> {
        event.validate()?;
        let mut state = self.state.lock().await;

        if let Some(existing) = state.events.get(&event.event_id) {
            return if existing == event {
                Ok(())
            } else {
                Err(MeteringError::DuplicateConflict(DuplicateIdentity::EventId))
            };
        }
        if let Some(existing_id) = state.idempotency.get(&event.idempotency_key) {
            return if state.events.get(existing_id) == Some(event) {
                Ok(())
            } else {
                Err(MeteringError::DuplicateConflict(
                    DuplicateIdentity::IdempotencyKey,
                ))
            };
        }

        state
            .idempotency
            .insert(event.idempotency_key.clone(), event.event_id);
        state.events.insert(event.event_id, event.clone());
        Ok(())
    }

    async fn current(
        &self,
        subject: &UsageSubject,
        dimension: &UsageDimension,
        window: TimeWindow,
    ) -> Result<UsageAggregate, MeteringError> {
        subject.validate()?;
        dimension.validate()?;
        window.validate()?;
        let now = Utc::now();
        let state = self.state.lock().await;
        let mut quantity = UsageQuantity::zero(dimension.unit())?;
        let mut unavailable_events = 0_u64;

        for event in state.events.values().filter(|event| {
            event.subject == *subject
                && event.dimension == *dimension
                && event.affects_actual_total()
                && window.contains(event.event_time, now)
        }) {
            match &event.measurement {
                UsageMeasurement::Measured(measured) => {
                    quantity = quantity.checked_add(measured)?;
                }
                UsageMeasurement::Unavailable { .. } => {
                    unavailable_events = unavailable_events.saturating_add(1);
                }
            }
        }

        Ok(UsageAggregate {
            quantity,
            unavailable_events,
        })
    }

    async fn flush(&self, _: Duration) -> Result<(), MeteringError> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuotaDecision {
    Allow {
        remaining: Option<UsageQuantity>,
        policy_id: String,
        reset_at: Option<DateTime<Utc>>,
    },
    SoftLimit {
        remaining: Option<UsageQuantity>,
        policy_id: String,
        reset_at: Option<DateTime<Utc>>,
    },
    HardLimit {
        policy_id: String,
        retry_after: Option<Duration>,
        reset_at: Option<DateTime<Utc>>,
    },
}

impl QuotaDecision {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Self::HardLimit { .. })
    }

    pub fn policy_id(&self) -> &str {
        match self {
            Self::Allow { policy_id, .. }
            | Self::SoftLimit { policy_id, .. }
            | Self::HardLimit { policy_id, .. } => policy_id,
        }
    }

    pub fn reset_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Allow { reset_at, .. }
            | Self::SoftLimit { reset_at, .. }
            | Self::HardLimit { reset_at, .. } => *reset_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuotaReservationRequest {
    pub reservation_id: Uuid,
    pub idempotency_key: String,
    pub subject: UsageSubject,
    pub dimension: UsageDimension,
    pub estimated: UsageQuantity,
    pub expires_at: DateTime<Utc>,
}

impl QuotaReservationRequest {
    pub fn validate(&self) -> Result<(), QuotaError> {
        if self.reservation_id.is_nil() {
            return Err(QuotaError::InvalidRequest);
        }
        validate_id_ref(&self.idempotency_key, "reservation idempotency key")?;
        self.subject.validate()?;
        self.dimension.validate()?;
        self.estimated.validate()?;
        if self.estimated.unit() != &self.dimension.unit() || self.estimated.is_negative() {
            return Err(QuotaError::InvalidRequest);
        }
        if self.expires_at <= Utc::now() {
            return Err(QuotaError::ReservationExpired);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuotaReservation {
    pub reservation_id: Uuid,
    pub idempotency_key: String,
    pub subject: UsageSubject,
    pub dimension: UsageDimension,
    pub policy_id: String,
    pub reserved: UsageQuantity,
    pub expires_at: DateTime<Utc>,
}

impl QuotaReservation {
    fn validate(&self) -> Result<(), QuotaError> {
        if self.reservation_id.is_nil() {
            return Err(QuotaError::InvalidRequest);
        }
        validate_id_ref(&self.idempotency_key, "reservation idempotency key")?;
        validate_label(&self.policy_id, "quota policy id")?;
        self.subject.validate()?;
        self.dimension.validate()?;
        self.reserved.validate()?;
        if self.reserved.unit() != &self.dimension.unit() || self.reserved.is_negative() {
            return Err(QuotaError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum QuotaError {
    #[error("invalid quota request")]
    InvalidRequest,
    #[error("quota state unavailable")]
    Unavailable,
    #[error("quota reservation not found")]
    ReservationNotFound,
    #[error("quota reservation conflicts with an existing reservation")]
    ReservationConflict,
    #[error("quota reservation expired")]
    ReservationExpired,
    #[error(transparent)]
    Metering(#[from] MeteringError),
}

#[async_trait]
pub trait QuotaPolicy: Send + Sync {
    /// Perform an advisory, read-only check.
    ///
    /// Consumers enforcing a spend or concurrency cap must use `reserve`,
    /// `finalize`, and `release`; a check followed by work is not atomic.
    async fn check(
        &self,
        subject: &UsageSubject,
        dimension: &UsageDimension,
        quantity: &UsageQuantity,
    ) -> Result<QuotaDecision, QuotaError>;

    /// Atomically reserve estimated capacity.
    ///
    /// An exact identity replay returns the original reservation; a conflicting
    /// replay returns `ReservationConflict`.
    async fn reserve(
        &self,
        request: &QuotaReservationRequest,
    ) -> Result<QuotaReservation, QuotaError>;

    /// Reconcile a reservation with actual usage exactly once.
    ///
    /// Replaying the same finalization must return the original decision;
    /// conflicting actuals must fail without changing reserved capacity.
    async fn finalize(
        &self,
        reservation: &QuotaReservation,
        actual: &UsageQuantity,
    ) -> Result<QuotaDecision, QuotaError>;

    async fn release(&self, reservation: &QuotaReservation) -> Result<(), QuotaError>;
}

#[derive(Clone, Debug, Default)]
pub struct UnlimitedQuota;

impl UnlimitedQuota {
    fn allow() -> QuotaDecision {
        QuotaDecision::Allow {
            remaining: None,
            policy_id: "unlimited".to_string(),
            reset_at: None,
        }
    }
}

#[async_trait]
impl QuotaPolicy for UnlimitedQuota {
    async fn check(
        &self,
        subject: &UsageSubject,
        dimension: &UsageDimension,
        quantity: &UsageQuantity,
    ) -> Result<QuotaDecision, QuotaError> {
        subject.validate()?;
        dimension.validate()?;
        quantity.validate()?;
        if quantity.unit() != &dimension.unit() || quantity.is_negative() {
            return Err(QuotaError::InvalidRequest);
        }
        Ok(Self::allow())
    }

    async fn reserve(
        &self,
        request: &QuotaReservationRequest,
    ) -> Result<QuotaReservation, QuotaError> {
        request.validate()?;
        Ok(QuotaReservation {
            reservation_id: request.reservation_id,
            idempotency_key: request.idempotency_key.clone(),
            subject: request.subject.clone(),
            dimension: request.dimension.clone(),
            policy_id: "unlimited".to_string(),
            reserved: request.estimated.clone(),
            expires_at: request.expires_at,
        })
    }

    async fn finalize(
        &self,
        reservation: &QuotaReservation,
        actual: &UsageQuantity,
    ) -> Result<QuotaDecision, QuotaError> {
        reservation.validate()?;
        actual.validate()?;
        if reservation.expires_at <= Utc::now() {
            return Err(QuotaError::ReservationExpired);
        }
        if actual.unit() != &reservation.dimension.unit() || actual.is_negative() {
            return Err(QuotaError::InvalidRequest);
        }
        Ok(Self::allow())
    }

    async fn release(&self, reservation: &QuotaReservation) -> Result<(), QuotaError> {
        reservation.validate()?;
        Ok(())
    }
}

fn validate_id(value: String, field: &'static str) -> Result<String, MeteringError> {
    validate_id_ref(&value, field)?;
    Ok(value)
}

fn validate_id_ref(value: &str, field: &'static str) -> Result<(), MeteringError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value.chars().any(char::is_control)
        || looks_secret_shaped(value)
    {
        return Err(MeteringError::InvalidIdentifier(field));
    }
    Ok(())
}

fn validate_label(value: &str, field: &'static str) -> Result<(), MeteringError> {
    if value.is_empty()
        || value.len() > MAX_LABEL_BYTES
        || value.contains("://")
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/' | b':')
        })
    {
        return Err(MeteringError::InvalidLabel(field));
    }
    Ok(())
}

fn looks_secret_shaped(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.contains("authorization=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("token=")
        || lower.contains("secret=")
        || lower.contains("password=")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    fn request_event(
        key: &str,
        subject: UsageSubject,
        quantity: u64,
    ) -> Result<UsageEvent, MeteringError> {
        UsageEvent::new(
            key,
            Utc::now(),
            subject,
            UsageDimension::ApiRequest,
            UsageMeasurement::Measured(UsageQuantity::whole(quantity, UsageUnit::Count)?),
            UsageClass::BillableActual,
            UsageProducer::Api,
            UsageSource::LocalMeasured,
            UsageOutcome::Completed,
        )
    }

    #[test]
    fn quantity_is_exact_and_unit_safe() {
        let left =
            UsageQuantity::new(BigDecimal::from_str("0.1").unwrap(), UsageUnit::Second).unwrap();
        let right =
            UsageQuantity::new(BigDecimal::from_str("0.2").unwrap(), UsageUnit::Second).unwrap();
        let total = left.checked_add(&right).unwrap();
        assert_eq!(total.value(), &BigDecimal::from_str("0.3").unwrap());
        assert_eq!(
            total.checked_add(&UsageQuantity::whole(1, UsageUnit::Count).unwrap()),
            Err(MeteringError::UnitMismatch)
        );
        assert_eq!(
            serde_json::to_value(&total).unwrap(),
            serde_json::json!({"value": "0.3", "unit": "second"})
        );
        assert!(
            UsageQuantity::new(BigDecimal::from_str("1e1000").unwrap(), UsageUnit::Count).is_err()
        );
    }

    #[test]
    fn attributes_are_dimension_allowlisted_and_label_bounded() {
        let mut attrs = UsageAttributes::default();
        assert!(attrs
            .insert(
                &UsageDimension::InferenceInputTokens,
                UsageAttributeKey::Model,
                UsageAttributeValue::label("gpt-5.1").unwrap(),
            )
            .is_ok());
        assert_eq!(
            attrs.insert(
                &UsageDimension::ApiRequest,
                UsageAttributeKey::Provider,
                UsageAttributeValue::label("openai").unwrap(),
            ),
            Err(MeteringError::AttributeNotAllowed(
                UsageAttributeKey::Provider
            ))
        );
        assert!(UsageAttributeValue::label("https://secret.example").is_err());
    }

    #[test]
    fn unavailable_measurement_never_becomes_zero() {
        let event = UsageEvent::new(
            "provider-call:unknown",
            Utc::now(),
            UsageSubject::unknown(),
            UsageDimension::InferenceOutputTokens,
            UsageMeasurement::Unavailable {
                unit: UsageUnit::Token,
            },
            UsageClass::BillableActual,
            UsageProducer::Inference,
            UsageSource::Unavailable,
            UsageOutcome::ProviderInterrupted,
        )
        .unwrap();
        assert!(event.measurement.quantity().is_none());
    }

    #[tokio::test]
    async fn duplicate_replay_is_idempotent_and_conflict_is_rejected() {
        let meter = InMemoryMeter::default();
        let event = request_event("request:one", UsageSubject::unknown(), 1).unwrap();
        meter.record(&event).await.unwrap();
        meter.record(&event).await.unwrap();
        assert_eq!(meter.events().await.len(), 1);

        let conflict = request_event("request:one", UsageSubject::unknown(), 2).unwrap();
        assert_eq!(
            meter.record(&conflict).await,
            Err(MeteringError::DuplicateConflict(
                DuplicateIdentity::IdempotencyKey
            ))
        );

        let event_id_conflict = request_event("request:two", UsageSubject::unknown(), 4)
            .unwrap()
            .with_identity(event.event_id, event.recorded_at);
        assert_eq!(
            meter.record(&event_id_conflict).await,
            Err(MeteringError::DuplicateConflict(DuplicateIdentity::EventId))
        );
    }

    #[tokio::test]
    async fn aggregate_separates_subjects_and_tracks_unknown_measurements() {
        let meter = InMemoryMeter::default();
        let anonymous = UsageSubject::anonymous("anon:one").unwrap();
        let unknown = UsageSubject::unknown();
        assert!(anonymous.is_anonymous());
        assert!(!anonymous.is_unknown());
        assert!(unknown.is_unknown());

        meter
            .record(&request_event("request:anonymous", anonymous.clone(), 2).unwrap())
            .await
            .unwrap();
        meter
            .record(&request_event("request:unknown", unknown.clone(), 7).unwrap())
            .await
            .unwrap();

        let aggregate = meter
            .current(
                &anonymous,
                &UsageDimension::ApiRequest,
                TimeWindow::Lifetime,
            )
            .await
            .unwrap();
        assert_eq!(aggregate.quantity.value(), &BigDecimal::from(2_u64));
    }

    #[tokio::test]
    async fn actuals_and_reversals_aggregate_without_estimates() {
        let meter = InMemoryMeter::default();
        let subject = UsageSubject::unknown().with_tenant("tenant:one").unwrap();
        let actual = request_event("request:actual", subject.clone(), 5).unwrap();
        let mut reversal = request_event("request:reversal", subject.clone(), 0).unwrap();
        reversal.measurement = UsageMeasurement::Measured(
            UsageQuantity::new(BigDecimal::from(-2_i64), UsageUnit::Count).unwrap(),
        );
        reversal.class = UsageClass::Reversal;
        reversal.outcome = UsageOutcome::Corrected;
        let mut estimate = request_event("request:estimate", subject.clone(), 100).unwrap();
        estimate.class = UsageClass::NonBillableEstimate;
        estimate.source = UsageSource::Estimated;

        meter.record(&actual).await.unwrap();
        meter.record(&reversal).await.unwrap();
        meter.record(&estimate).await.unwrap();

        let aggregate = meter
            .current(&subject, &UsageDimension::ApiRequest, TimeWindow::Lifetime)
            .await
            .unwrap();
        assert_eq!(aggregate.quantity.value(), &BigDecimal::from(3_u64));
    }

    #[test]
    fn debug_output_does_not_expose_subject_or_correlation_values() {
        let event = request_event(
            "request:debug",
            UsageSubject::unknown()
                .with_tenant("tenant-sensitive")
                .unwrap(),
            1,
        )
        .unwrap()
        .with_correlation(
            UsageCorrelation::default()
                .with_provider_call_id("provider-sensitive")
                .unwrap(),
        )
        .unwrap();
        let debug = format!("{event:?}");
        assert!(!debug.contains("tenant-sensitive"));
        assert!(!debug.contains("provider-sensitive"));
        assert!(!debug.contains("request:debug"));
    }

    #[tokio::test]
    async fn deserialized_events_revalidate_identity_and_private_fields() {
        let event = request_event("request:persisted", UsageSubject::unknown(), 1).unwrap();
        let mut encoded = serde_json::to_value(&event).unwrap();
        encoded["subject"]["tenant_id"] = serde_json::json!("api_key=secret-value");
        let decoded: UsageEvent = serde_json::from_value(encoded).unwrap();
        assert_eq!(
            NoOpMeter.record(&decoded).await,
            Err(MeteringError::InvalidIdentifier("tenant id"))
        );

        let nil_identity = event.with_identity(Uuid::nil(), Utc::now());
        assert_eq!(
            NoOpMeter.record(&nil_identity).await,
            Err(MeteringError::InvalidIdentifier("event id"))
        );
    }

    #[tokio::test]
    async fn invalid_time_ranges_are_rejected() {
        let now = Utc::now();
        assert_eq!(
            NoOpMeter
                .current(
                    &UsageSubject::unknown(),
                    &UsageDimension::ApiRequest,
                    TimeWindow::Range {
                        start: now,
                        end: now,
                    },
                )
                .await,
            Err(MeteringError::InvalidTimeWindow)
        );
    }

    #[test]
    fn quota_decisions_preserve_soft_hard_and_reset_metadata() {
        let reset_at = Utc::now() + chrono::Duration::minutes(1);
        let soft = QuotaDecision::SoftLimit {
            remaining: Some(UsageQuantity::whole(3, UsageUnit::Count).unwrap()),
            policy_id: "plan:soft".to_string(),
            reset_at: Some(reset_at),
        };
        let hard = QuotaDecision::HardLimit {
            policy_id: "plan:hard".to_string(),
            retry_after: Some(Duration::from_secs(60)),
            reset_at: Some(reset_at),
        };
        assert!(soft.is_allowed());
        assert!(!hard.is_allowed());
        assert_eq!(soft.reset_at(), Some(reset_at));
        assert_eq!(hard.policy_id(), "plan:hard");
    }

    #[tokio::test]
    async fn unlimited_quota_supports_reservation_lifecycle_for_unknown_subject() {
        let policy = UnlimitedQuota;
        let request = QuotaReservationRequest {
            reservation_id: Uuid::new_v4(),
            idempotency_key: "reservation:one".to_string(),
            subject: UsageSubject::unknown(),
            dimension: UsageDimension::InferenceInputTokens,
            estimated: UsageQuantity::whole(100, UsageUnit::Token).unwrap(),
            expires_at: Utc::now() + chrono::Duration::minutes(1),
        };
        let reservation = policy.reserve(&request).await.unwrap();
        assert_eq!(reservation.reservation_id, request.reservation_id);
        assert!(policy
            .finalize(
                &reservation,
                &UsageQuantity::whole(80, UsageUnit::Token).unwrap()
            )
            .await
            .unwrap()
            .is_allowed());
        policy.release(&reservation).await.unwrap();
    }

    #[tokio::test]
    async fn expired_reservations_are_rejected() {
        let policy = UnlimitedQuota;
        let request = QuotaReservationRequest {
            reservation_id: Uuid::new_v4(),
            idempotency_key: "reservation:expired".to_string(),
            subject: UsageSubject::unknown(),
            dimension: UsageDimension::ApiRequest,
            estimated: UsageQuantity::whole(1, UsageUnit::Count).unwrap(),
            expires_at: Utc::now() - chrono::Duration::seconds(1),
        };
        assert_eq!(
            policy.reserve(&request).await,
            Err(QuotaError::ReservationExpired)
        );
    }

    #[test]
    fn metering_traits_are_object_safe() {
        let _meter: Arc<dyn UsageMeter> = Arc::new(NoOpMeter);
        let _quota: Arc<dyn QuotaPolicy> = Arc::new(UnlimitedQuota);
    }
}
