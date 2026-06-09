//! Analytics Tauri commands.
//!
//! The single backend path for frontend-originated PostHog feature events. Thin pass-through to
//! [`crate::analytics::posthog::capture`] so the PostHog key, the install id, the consent gate, and
//! the person-properties all stay backend (smart backend, thin frontend). See `analytics/CLAUDE.md`
//! § "PostHog feature events".

/// Records a frontend-originated PostHog feature event. Fire-and-forget: returns immediately, and
/// the underlying [`capture`](crate::analytics::posthog::capture) is gated (consent + dev/CI
/// suppression + missing-key no-op) so this is safe to call unconditionally from the frontend.
///
/// `props_json` is a JSON-encoded object of PII-free props (enums, counts, bools only; never paths,
/// names, queries, or prompts). It's a string, not a structured type, because the event prop set is
/// open and `serde_json::Value` can't cross the specta IPC boundary; the frontend's typed
/// `trackEvent` wrapper does the `JSON.stringify`. A malformed or non-object `props_json` degrades
/// to no props (the event still fires with just `source: "desktop"`).
#[tauri::command]
#[specta::specta]
pub async fn track_event(name: String, props_json: String) {
    let props = serde_json::from_str::<serde_json::Value>(&props_json)
        .ok()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    crate::analytics::posthog::capture(&name, props);
}
