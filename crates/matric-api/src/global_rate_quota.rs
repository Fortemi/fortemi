//! Convert the global requests-per-period configuration into a governor quota.

use std::num::NonZeroU32;
use std::time::Duration;

use governor::Quota;

pub(crate) fn quota_for_requests(requests: u32, period_secs: u64) -> anyhow::Result<Quota> {
    let requests = NonZeroU32::new(requests)
        .ok_or_else(|| anyhow::anyhow!("RATE_LIMIT_REQUESTS must be greater than zero"))?;
    if period_secs == 0 {
        anyhow::bail!("RATE_LIMIT_PERIOD_SECS must be greater than zero");
    }

    // governor 0.6.3's with_period replenishes ONE token per interval;
    // allow_burst only sets capacity. Round division up to a whole nanosecond
    // so truncation can never increase the configured sustained rate.
    let period = Duration::from_secs(period_secs);
    let mut interval = period / requests.get();
    if interval * requests.get() < period {
        interval += Duration::from_nanos(1);
    }

    // governor stores both interval and burst tolerance in u64 nanoseconds.
    // Reserve one extra interval for its initial theoretical arrival time too.
    let interval_ns = u64::try_from(interval.as_nanos()).map_err(|_| {
        anyhow::anyhow!(
            "RATE_LIMIT_PERIOD_SECS / RATE_LIMIT_REQUESTS exceeds governor's time range"
        )
    })?;
    if interval_ns
        .checked_mul(u64::from(requests.get()) + 1)
        .is_none()
    {
        anyhow::bail!(
            "RATE_LIMIT_REQUESTS and RATE_LIMIT_PERIOD_SECS exceed governor's time range"
        );
    }

    let quota = Quota::with_period(interval)
        .ok_or_else(|| anyhow::anyhow!("rate limit refill interval must be greater than zero"))?;
    Ok(quota.allow_burst(requests))
}

#[cfg(test)]
mod tests {
    use super::quota_for_requests;
    use governor::clock::{Clock, FakeRelativeClock};
    use governor::RateLimiter;
    use std::time::Duration;

    #[test]
    fn six_hundred_per_minute_preserves_burst_and_refills_across_two_periods() {
        let quota = quota_for_requests(600, 60).unwrap();
        assert_eq!(quota.burst_size().get(), 600);
        assert_eq!(quota.replenish_interval(), Duration::from_millis(100));
        let clock = FakeRelativeClock::default();
        let limiter = RateLimiter::direct_with_clock(quota, &clock);

        for _ in 0..600 {
            assert!(limiter.check().is_ok());
        }
        let denied = limiter.check().expect_err("request 601 must be refused");
        assert_eq!(
            denied.wait_time_from(clock.now()),
            Duration::from_millis(100)
        );

        // Exhausted capacity admits exactly one request every 100ms, including
        // the 60s and 120s boundaries; each period admits another 600 requests.
        for period in 1..=2 {
            for _ in 0..600 {
                clock.advance(Duration::from_nanos(99_999_999));
                assert!(limiter.check().is_err());
                clock.advance(Duration::from_nanos(1));
                assert!(limiter.check().is_ok());
                assert!(limiter.check().is_err());
            }
            assert_eq!(
                Duration::from(clock.now()),
                Duration::from_secs(period * 60)
            );
        }
    }

    #[test]
    fn single_request_quota_refills_only_after_the_full_period() {
        let quota = quota_for_requests(1, 60).unwrap();
        assert_eq!(quota.burst_size().get(), 1);
        assert_eq!(quota.replenish_interval(), Duration::from_secs(60));
        let clock = FakeRelativeClock::default();
        let limiter = RateLimiter::direct_with_clock(quota, &clock);
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
        clock.advance(Duration::from_nanos(59_999_999_999));
        assert!(limiter.check().is_err());
        clock.advance(Duration::from_nanos(1));
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
    }

    #[test]
    fn fractional_nanoseconds_round_up_without_exceeding_the_rate() {
        let quota = quota_for_requests(3, 1).unwrap();
        assert_eq!(quota.burst_size().get(), 3);
        assert_eq!(
            quota.replenish_interval(),
            Duration::from_nanos(333_333_334)
        );
        let clock = FakeRelativeClock::default();
        let limiter = RateLimiter::direct_with_clock(quota, &clock);
        assert!(limiter.check_n(quota.burst_size()).unwrap().is_ok());
        assert!(limiter.check().is_err());

        clock.advance(Duration::from_nanos(333_333_333));
        assert!(limiter.check().is_err());
        clock.advance(Duration::from_nanos(1));
        assert!(limiter.check().is_ok());
        clock.advance(Duration::from_nanos(666_666_666));
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
        clock.advance(Duration::from_nanos(2));
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
    }

    #[test]
    fn existing_configuration_extremes_are_representable() {
        for (requests, period_secs) in [
            (1, crate::MAX_RATE_LIMIT_PERIOD_SECS),
            (crate::MAX_RATE_LIMIT_REQUESTS, 1),
            (
                crate::MAX_RATE_LIMIT_REQUESTS,
                crate::MAX_RATE_LIMIT_PERIOD_SECS,
            ),
        ] {
            let quota = quota_for_requests(requests, period_secs).unwrap();
            assert_eq!(quota.burst_size().get(), requests);
            let clock = FakeRelativeClock::default();
            let limiter = RateLimiter::direct_with_clock(quota, &clock);
            assert!(limiter.check_n(quota.burst_size()).unwrap().is_ok());
            assert!(limiter.check().is_err());
            clock.advance(quota.replenish_interval());
            assert!(limiter.check().is_ok());
            assert!(limiter.check().is_err());
        }
    }

    #[test]
    fn subnanosecond_rates_round_up_to_one_nanosecond() {
        let quota = quota_for_requests(u32::MAX, 1).unwrap();
        assert_eq!(quota.burst_size().get(), u32::MAX);
        assert_eq!(quota.replenish_interval(), Duration::from_nanos(1));
        let clock = FakeRelativeClock::default();
        let limiter = RateLimiter::direct_with_clock(quota, &clock);
        assert!(limiter.check_n(quota.burst_size()).unwrap().is_ok());
        assert!(limiter.check().is_err());
        clock.advance(Duration::from_nanos(1));
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
    }

    #[test]
    fn zero_and_unrepresentable_configuration_is_rejected() {
        assert!(quota_for_requests(0, 60)
            .unwrap_err()
            .to_string()
            .contains("RATE_LIMIT_REQUESTS"));
        assert!(quota_for_requests(600, 0)
            .unwrap_err()
            .to_string()
            .contains("RATE_LIMIT_PERIOD_SECS"));
        assert!(quota_for_requests(1, u64::MAX).is_err());
        // The interval fits u64 nanos, but governor's initial state does not.
        assert!(quota_for_requests(1, u64::MAX / 1_000_000_000).is_err());
        // Each interval fits; burst tolerance exceeds governor's u64 range.
        assert!(quota_for_requests(3, u64::MAX / 1_000_000_000 + 1).is_err());
    }
}
