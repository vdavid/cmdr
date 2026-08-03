//! The app's answer to a backend's PII-free product counters.
//!
//! A storage backend reports that something happened and knows nothing about
//! consent, dev/CI suppression, or PostHog. It asks through
//! `cmdr_fs::volume::host::analytics::AnalyticsSink`; this hands the event to
//! [`posthog::capture`](super::posthog::capture), which owns every one of those
//! gates.
//!
//! ❌ The seam takes `&[(&str, &str)]` so a struct can't slip through and carry
//! a hostname, a share, or a path into an event. Nothing here widens that: the
//! pairs become a flat JSON object and nothing else.

use serde_json::{Map, Value};

use cmdr_fs::volume::host::analytics::AnalyticsSink;

/// Sends a backend's counters through the app's consent-gated analytics client.
pub struct PostHogVolumeAnalytics;

/// The seam's string pairs as the flat object PostHog takes.
fn properties_to_json(properties: &[(&str, &str)]) -> Value {
    Value::Object(
        properties
            .iter()
            .map(|(name, value)| ((*name).to_string(), Value::String((*value).to_string())))
            .collect::<Map<_, _>>(),
    )
}

impl AnalyticsSink for PostHogVolumeAnalytics {
    fn record(&self, event: &str, properties: &[(&str, &str)]) {
        super::posthog::capture(event, properties_to_json(properties));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat strings in, flat strings out. A nested value would be a property
    /// nobody could have guessed in advance, which is the PII rule's whole test.
    #[test]
    fn properties_become_a_flat_object_of_strings() {
        let json = properties_to_json(&[("transport", "direct"), ("outcome", "fallback")]);
        assert_eq!(json["transport"], "direct");
        assert_eq!(json["outcome"], "fallback");
        assert_eq!(json.as_object().expect("an object").len(), 2);
    }

    #[test]
    fn an_event_with_no_properties_is_an_empty_object() {
        assert_eq!(properties_to_json(&[]), serde_json::json!({}));
    }
}
