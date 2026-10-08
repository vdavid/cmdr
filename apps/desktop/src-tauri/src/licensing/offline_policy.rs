//! What a license is worth when the server can't be asked. Pure, so every hostile case is a test.
//!
//! The one principle: unreachability alone never downgrades a valid signed license.
//!
//! - **Perpetual**: valid forever on the strength of its signature. Only a verified `invalid` (a
//!   revocation) or `expired` answer from the server changes that, never the age of the last answer.
//! - **Dated key** (an `expiresAt` signed into the key): valid until that date, offline or not.
//! - **Renewing subscription** (no date in the key): valid until the last server-reported period
//!   end plus [`RENEWAL_GRACE_SECS`], so a renewal the app hasn't seen yet doesn't expire it. A
//!   verified `active` also counts for [`UNCONFIRMED_TERM_GRACE_SECS`] from when it arrived.
//! - **Time-limited with no known end at all** (activated offline, never confirmed):
//!   [`UNCONFIRMED_TERM_GRACE_SECS`] from activation, then Personal. The only case that needs the
//!   server, and only subscriptions, which are no longer sold, can reach it.

use crate::licensing::app_status::{CachedLicenseStatus, LicenseType};

/// How long past a renewing subscription's last known period end it stays valid (30 days).
pub(super) const RENEWAL_GRACE_SECS: u64 = 30 * 24 * 60 * 60;

/// How long a time-limited license with no known end date stays valid after its last confirmation
/// (or activation): 30 days.
pub(super) const UNCONFIRMED_TERM_GRACE_SECS: u64 = 30 * 24 * 60 * 60;

/// What the signed key itself says about its term.
#[derive(Debug, Clone, Default)]
pub(super) struct KeyTerms {
    pub license_type: Option<LicenseType>,
    /// ISO 8601, signed into the key. Only a dated license carries one.
    pub expires_at: Option<String>,
}

/// The license verdict, before the UI flags (reminder, expiry modal) are added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LicenseState {
    Active {
        license_type: LicenseType,
        expires_at: Option<String>,
    },
    Expired {
        expired_at: String,
    },
    Unlicensed,
}

/// Decide the license state from the signed key, the last cached server answer, and the time.
///
/// `now` is the clock-guarded time (`app_status::effective_now`), so setting the Mac's clock back
/// doesn't stretch a time-limited license.
pub(super) fn resolve_license_state(key: &KeyTerms, cached: Option<&CachedLicenseStatus>, now: u64) -> LicenseState {
    // A server answer is authoritative about status. `invalid` is a revocation (or a key the server
    // never knew); anything we don't recognize is not trusted as a license either.
    match cached.map(|c| c.status.as_str()) {
        Some("active") | None => {}
        Some("expired") => {
            let expired_at = cached
                .and_then(|c| c.expires_at.clone())
                .or_else(|| key.expires_at.clone())
                .unwrap_or_else(|| "unknown".to_string());
            return LicenseState::Expired { expired_at };
        }
        Some(_) => return LicenseState::Unlicensed,
    }

    // The server's word on the type wins over the key's, as it always has; a legacy key with no
    // commercial type has neither.
    let Some(license_type) = cached.and_then(|c| c.license_type).or(key.license_type) else {
        return LicenseState::Unlicensed;
    };

    match license_type {
        LicenseType::CommercialPerpetual => LicenseState::Active {
            license_type,
            expires_at: cached.and_then(|c| c.expires_at.clone()),
        },
        LicenseType::CommercialSubscription => resolve_time_limited(key, cached, now),
    }
}

fn resolve_time_limited(key: &KeyTerms, cached: Option<&CachedLicenseStatus>, now: u64) -> LicenseState {
    let active = |expires_at: Option<String>| LicenseState::Active {
        license_type: LicenseType::CommercialSubscription,
        expires_at,
    };

    // 1. A date signed into the key is the end, full stop.
    if let Some((raw, end)) = key
        .expires_at
        .as_deref()
        .and_then(|raw| parse_timestamp(raw).map(|end| (raw, end)))
    {
        return if now >= end {
            LicenseState::Expired {
                expired_at: raw.to_string(),
            }
        } else {
            active(Some(raw.to_string()))
        };
    }

    let Some(cached) = cached else {
        return LicenseState::Unlicensed;
    };
    let confirmed_until = cached.cached_at.saturating_add(UNCONFIRMED_TERM_GRACE_SECS);

    // 2. A renewing subscription runs to the last reported period end plus the renewal grace, and
    //    a recent `active` answer counts on its own (a subscription in dunning reports a past end).
    if let Some((raw, end)) = cached
        .expires_at
        .as_deref()
        .and_then(|raw| parse_timestamp(raw).map(|end| (raw, end)))
    {
        let valid_until = end.saturating_add(RENEWAL_GRACE_SECS).max(confirmed_until);
        return if now >= valid_until {
            LicenseState::Expired {
                expired_at: raw.to_string(),
            }
        } else {
            active(Some(raw.to_string()))
        };
    }

    // 3. No known end at all: good for the unconfirmed grace from the last confirmation.
    if now < confirmed_until {
        active(None)
    } else {
        LicenseState::Unlicensed
    }
}

/// Unix seconds from an RFC 3339 timestamp, or `None` if it doesn't parse (or predates 1970).
pub(super) fn parse_timestamp(raw: &str) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    u64::try_from(parsed.timestamp()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;
    /// 2026-10-05T00:00:00Z
    const NOW: u64 = 1_791_158_400;

    fn perpetual_key() -> KeyTerms {
        KeyTerms {
            license_type: Some(LicenseType::CommercialPerpetual),
            expires_at: None,
        }
    }

    fn subscription_key(expires_at: Option<&str>) -> KeyTerms {
        KeyTerms {
            license_type: Some(LicenseType::CommercialSubscription),
            expires_at: expires_at.map(str::to_string),
        }
    }

    fn cached(
        status: &str,
        license_type: Option<LicenseType>,
        expires_at: Option<&str>,
        cached_at: u64,
    ) -> CachedLicenseStatus {
        CachedLicenseStatus {
            status: status.to_string(),
            license_type,
            organization_name: None,
            expires_at: expires_at.map(str::to_string),
            cached_at,
        }
    }

    fn active_perpetual() -> LicenseState {
        LicenseState::Active {
            license_type: LicenseType::CommercialPerpetual,
            expires_at: None,
        }
    }

    // Perpetual

    #[test]
    fn a_perpetual_license_stays_valid_after_years_without_reaching_the_server() {
        let last_answer = cached(
            "active",
            Some(LicenseType::CommercialPerpetual),
            None,
            NOW - 3 * 365 * DAY,
        );

        assert_eq!(
            resolve_license_state(&perpetual_key(), Some(&last_answer), NOW),
            active_perpetual()
        );
    }

    #[test]
    fn a_perpetual_license_needs_no_server_answer_at_all() {
        assert_eq!(resolve_license_state(&perpetual_key(), None, NOW), active_perpetual());
    }

    #[test]
    fn a_perpetual_license_ignores_the_clock_entirely() {
        let last_answer = cached("active", Some(LicenseType::CommercialPerpetual), None, NOW);

        assert_eq!(
            resolve_license_state(&perpetual_key(), Some(&last_answer), 0),
            active_perpetual()
        );
        assert_eq!(
            resolve_license_state(&perpetual_key(), Some(&last_answer), u64::MAX),
            active_perpetual()
        );
    }

    #[test]
    fn a_verified_revocation_drops_a_perpetual_license_to_personal() {
        let revoked = cached("invalid", None, None, NOW - 400 * DAY);

        assert_eq!(
            resolve_license_state(&perpetual_key(), Some(&revoked), NOW),
            LicenseState::Unlicensed
        );
    }

    #[test]
    fn a_verified_expiry_is_honored_even_for_a_perpetual_key() {
        let expired = cached(
            "expired",
            Some(LicenseType::CommercialPerpetual),
            Some("2026-01-01T00:00:00Z"),
            NOW,
        );

        assert_eq!(
            resolve_license_state(&perpetual_key(), Some(&expired), NOW),
            LicenseState::Expired {
                expired_at: "2026-01-01T00:00:00Z".to_string()
            }
        );
    }

    #[test]
    fn an_unknown_server_status_is_not_trusted_as_a_license() {
        let odd = cached("suspended", Some(LicenseType::CommercialPerpetual), None, NOW);

        assert_eq!(
            resolve_license_state(&perpetual_key(), Some(&odd), NOW),
            LicenseState::Unlicensed
        );
    }

    #[test]
    fn a_key_with_no_commercial_type_is_personal() {
        // Legacy "supporter" keys carry a type the app doesn't map.
        let key = KeyTerms::default();

        assert_eq!(resolve_license_state(&key, None, NOW), LicenseState::Unlicensed);
    }

    // Dated keys

    #[test]
    fn a_dated_key_is_valid_until_its_signed_date_with_no_server() {
        let key = subscription_key(Some("2026-12-31T23:59:59Z"));

        assert_eq!(
            resolve_license_state(&key, None, NOW),
            LicenseState::Active {
                license_type: LicenseType::CommercialSubscription,
                expires_at: Some("2026-12-31T23:59:59Z".to_string()),
            }
        );
    }

    #[test]
    fn a_dated_key_expires_on_its_signed_date_even_with_a_fresh_active_answer() {
        let key = subscription_key(Some("2026-10-01T00:00:00Z"));
        let fresh = cached("active", Some(LicenseType::CommercialSubscription), None, NOW);

        assert_eq!(
            resolve_license_state(&key, Some(&fresh), NOW),
            LicenseState::Expired {
                expired_at: "2026-10-01T00:00:00Z".to_string()
            }
        );
    }

    // Renewing subscriptions

    #[test]
    fn a_subscription_runs_through_its_reported_period_end_plus_the_renewal_grace() {
        let key = subscription_key(None);
        let answer = cached(
            "active",
            Some(LicenseType::CommercialSubscription),
            Some("2026-10-01T00:00:00Z"),
            NOW - 200 * DAY,
        );

        let within_grace = resolve_license_state(&key, Some(&answer), NOW + 20 * DAY);
        let past_grace = resolve_license_state(&key, Some(&answer), NOW + 40 * DAY);

        assert!(matches!(within_grace, LicenseState::Active { .. }));
        assert_eq!(
            past_grace,
            LicenseState::Expired {
                expired_at: "2026-10-01T00:00:00Z".to_string()
            }
        );
    }

    #[test]
    fn a_fresh_active_answer_counts_even_when_the_reported_period_already_ended() {
        // Paddle calls a subscription in dunning `past_due`, which the server reports as active.
        let key = subscription_key(None);
        let answer = cached(
            "active",
            Some(LicenseType::CommercialSubscription),
            Some("2026-07-01T00:00:00Z"),
            NOW - DAY,
        );

        assert!(matches!(
            resolve_license_state(&key, Some(&answer), NOW),
            LicenseState::Active { .. }
        ));
    }

    #[test]
    fn a_subscription_with_no_known_end_lasts_the_unconfirmed_grace_from_activation() {
        let key = subscription_key(None);
        let activated = cached(
            "active",
            Some(LicenseType::CommercialSubscription),
            None,
            NOW - 10 * DAY,
        );
        let activated_long_ago = cached(
            "active",
            Some(LicenseType::CommercialSubscription),
            None,
            NOW - 31 * DAY,
        );

        assert!(matches!(
            resolve_license_state(&key, Some(&activated), NOW),
            LicenseState::Active { .. }
        ));
        assert_eq!(
            resolve_license_state(&key, Some(&activated_long_ago), NOW),
            LicenseState::Unlicensed
        );
        assert_eq!(resolve_license_state(&key, None, NOW), LicenseState::Unlicensed);
    }

    #[test]
    fn an_unreadable_signed_date_falls_back_to_the_unconfirmed_grace() {
        let key = subscription_key(Some("not a date"));
        let activated = cached("active", Some(LicenseType::CommercialSubscription), None, NOW - DAY);

        assert!(matches!(
            resolve_license_state(&key, Some(&activated), NOW),
            LicenseState::Active { .. }
        ));
    }
}
