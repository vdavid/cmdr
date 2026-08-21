//! PII-free product counters.
//!
//! The narrowest seam here, and the one with the strictest rule. A backend
//! reports that something happened — a direct connection succeeded, a protocol
//! fell back to a slower path — so the product can tell whether a feature is
//! being used at all. The host decides whether to send anything: consent, dev
//! and CI suppression, and batching are all its business, and a backend must
//! behave identically whether the counter goes anywhere or not.
//!
//! ## ❌ Nothing identifying, ever
//!
//! No hostname, path, share, bucket, filename, username, credential, or IP
//! address may appear in a name or a property — not hashed, not truncated, not
//! "just the domain". Properties are short fixed strings a reader could have
//! guessed in advance: `("transport", "direct")`, `("outcome", "fallback")`.
//!
//! The `&[(&str, &str)]` shape is part of that guarantee: there's no way to hand
//! this seam a struct and hope its serialization is clean. If a property needs a
//! number, format it at the call site and think about whether the number itself
//! identifies anyone.

/// Where PII-free product counters go.
///
/// Cmdr answers this from the app's analytics layer, behind the user's consent
/// setting; a test or a tool answers nothing (`NoAnalytics`).
pub trait AnalyticsSink: Send + Sync {
    /// Records that `event` happened, with `properties` describing it.
    ///
    /// Fire-and-forget and never blocking: a backend calls it on whatever thread
    /// it's already on and moves on. ❌ Don't call it per entry, per chunk, or
    /// per retry — a counter is for a thing that happens when a user does
    /// something, not for measuring a loop.
    fn record(&self, event: &str, properties: &[(&str, &str)]);
}

/// Counters go nowhere.
pub(super) struct NoAnalytics;

impl AnalyticsSink for NoAnalytics {
    fn record(&self, _event: &str, _properties: &[(&str, &str)]) {}
}

#[cfg(any(test, feature = "testing"))]
pub use recording::{RecordedEvent, RecordingAnalytics};

#[cfg(any(test, feature = "testing"))]
mod recording {
    use std::sync::Mutex;

    use super::AnalyticsSink;
    use crate::ignore_poison::IgnorePoison;

    /// One recorded event: its name, and the properties that rode along.
    pub type RecordedEvent = (String, Vec<(String, String)>);

    /// An [`AnalyticsSink`] that remembers what it was
    /// handed, so a test can assert both that a counter fired and that nothing
    /// identifying rode along with it.
    #[derive(Default)]
    pub struct RecordingAnalytics {
        events: Mutex<Vec<RecordedEvent>>,
    }

    impl RecordingAnalytics {
        /// A recorder with nothing seen yet.
        pub fn new() -> Self {
            Self::default()
        }

        /// Every event recorded so far, in order, with its properties.
        pub fn events(&self) -> Vec<RecordedEvent> {
            self.events.lock_ignore_poison().clone()
        }
    }

    impl AnalyticsSink for RecordingAnalytics {
        fn record(&self, event: &str, properties: &[(&str, &str)]) {
            let properties = properties
                .iter()
                .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
                .collect();
            self.events.lock_ignore_poison().push((event.to_string(), properties));
        }
    }
}
