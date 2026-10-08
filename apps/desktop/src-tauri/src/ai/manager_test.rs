//! Tests for the AI manager facade: backend resolution (managed policy, consent, setup) and the
//! pure status decision.

use super::*;
use crate::managed_policy::testing::{self, ALLOWED_CLOUD_AI_HOSTS, DISABLE_AI, DISABLE_CLOUD_AI};
use crate::managed_policy::{ManagedAiRefusal, ManagedPolicy};

// --- managed policy: decided before consent, key, and endpoint ---

fn hosts(entries: &[&str]) -> ManagedPolicy {
    let list = entries.iter().map(|e| plist::Value::String((*e).to_string())).collect();
    testing::from_values(&[(ALLOWED_CLOUD_AI_HOSTS, plist::Value::Array(list))])
}

/// A fully set-up provider of each kind, under `policy`, with or without cloud consent.
fn resolve_under(policy: &ManagedPolicy, provider: &str, consented: bool) -> BackendResolution {
    resolve_backend_inner(
        policy,
        provider,
        Some(8080),
        String::from("sk-key"),
        String::from("https://api.openai.com/v1"),
        String::from("gpt-4o-mini"),
        true,
        consented,
    )
}

fn refusal(resolution: BackendResolution) -> Option<ManagedAiRefusal> {
    match resolution {
        BackendResolution::Managed(refusal) => Some(refusal),
        _ => None,
    }
}

#[test]
fn ai_off_refuses_every_provider_whatever_the_setup() {
    let policy = testing::forcing(&[DISABLE_AI]);
    for provider in ["local", "cloud", "off", "bogus"] {
        for consented in [true, false] {
            assert_eq!(
                refusal(resolve_under(&policy, provider, consented)),
                Some(ManagedAiRefusal::AiOff),
                "{provider}"
            );
        }
    }
}

#[test]
fn cloud_off_refuses_cloud_before_consent_and_keeps_local() {
    let policy = testing::forcing(&[DISABLE_CLOUD_AI]);
    assert_eq!(
        refusal(resolve_under(&policy, "cloud", false)),
        Some(ManagedAiRefusal::CloudAiOff)
    );
    assert_eq!(
        refusal(resolve_under(&policy, "cloud", true)),
        Some(ManagedAiRefusal::CloudAiOff)
    );
    assert!(matches!(
        resolve_under(&policy, "local", false),
        BackendResolution::Ready(_)
    ));
}

#[test]
fn a_host_list_judges_the_configured_endpoint() {
    assert_eq!(
        refusal(resolve_under(&hosts(&["*.openai.azure.com"]), "cloud", true)),
        Some(ManagedAiRefusal::HostNotAllowed)
    );
    assert!(matches!(
        resolve_under(&hosts(&["api.openai.com"]), "cloud", true),
        BackendResolution::Ready(_)
    ));
    // Still the managed reason, not "allow cloud AI", when consent is missing too.
    assert_eq!(
        refusal(resolve_under(&hosts(&["*.openai.azure.com"]), "cloud", false)),
        Some(ManagedAiRefusal::HostNotAllowed)
    );
}

#[test]
fn a_managed_refusal_maps_to_its_translate_kind_and_a_quiet_empty() {
    let Err(err) = BackendResolution::Managed(ManagedAiRefusal::CloudAiOff).into_translate_result() else {
        panic!("a refused resolution must not translate to a backend");
    };
    assert_eq!(err.kind, AiTranslateErrorKind::Managed);
    assert_eq!(err.managed, Some(ManagedAiRefusal::CloudAiOff));
    assert!(
        BackendResolution::Managed(ManagedAiRefusal::AiOff)
            .ready_or_log("test")
            .is_none()
    );
}

#[test]
fn a_cloud_only_feature_names_the_organization_when_no_cloud_host_is_allowed() {
    assert_eq!(
        cloud_only_refusal(&ManagedPolicy::default()).kind,
        AiTranslateErrorKind::NotConfigured
    );
    let refused = cloud_only_refusal(&testing::forcing(&[DISABLE_CLOUD_AI]));
    assert_eq!(refused.kind, AiTranslateErrorKind::Managed);
    assert_eq!(refused.managed, Some(ManagedAiRefusal::CloudAiOff));
}

#[test]
fn ai_off_never_offers_the_local_model() {
    let off = testing::forcing(&[DISABLE_AI]);
    assert_eq!(
        compute_ai_status(&off, "local", false, false, None, true, NOW),
        AiStatus::Unavailable
    );
    assert_eq!(
        compute_ai_status(&off, "local", true, true, None, true, NOW),
        AiStatus::Unavailable
    );
    let local_only = testing::forcing(&[DISABLE_CLOUD_AI]);
    assert_eq!(
        compute_ai_status(&local_only, "local", false, false, None, true, NOW),
        AiStatus::Offer
    );
}

#[test]
fn resolve_off_and_unknown_provider() {
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "off",
            None,
            String::new(),
            String::new(),
            String::new(),
            true,
            true
        ),
        BackendResolution::Off
    ));
    assert!(matches!(
        resolve_backend_inner(&ManagedPolicy::default(), "bogus", None, String::new(), String::new(), String::new(), true, true),
        BackendResolution::UnknownProvider(p) if p == "bogus"
    ));
}

#[test]
fn resolve_local_needs_a_running_port() {
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "local",
            None,
            String::new(),
            String::new(),
            String::new(),
            false,
            true
        ),
        BackendResolution::NotConfigured(_)
    ));
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "local",
            Some(8080),
            String::new(),
            String::new(),
            String::new(),
            false,
            true
        ),
        BackendResolution::Ready(_)
    ));
}

#[test]
fn resolve_cloud_key_required_provider_needs_a_key() {
    // A key-requiring provider (OpenAI etc.) with no key stays NotConfigured (friendly hint).
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "cloud",
            None,
            String::new(),
            String::from("https://api.openai.com/v1"),
            String::from("gpt-4o-mini"),
            true,
            true,
        ),
        BackendResolution::NotConfigured(_)
    ));
    // Same provider, key present → Ready.
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "cloud",
            None,
            String::from("sk-key"),
            String::from("https://api.openai.com/v1"),
            String::from("gpt-4o-mini"),
            true,
            true,
        ),
        BackendResolution::Ready(_)
    ));
}

#[test]
fn resolve_cloud_keyless_local_endpoint_is_ready() {
    // Ollama / LM Studio / custom: `requires_api_key = false`, no key, but a real endpoint +
    // model. This is the bug from issue #29 — it must resolve to Ready, not NotConfigured.
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "cloud",
            None,
            String::new(),
            String::from("http://localhost:11434/v1"),
            String::from("llama3.2"),
            false,
            true,
        ),
        BackendResolution::Ready(_)
    ));
    // A keyless *remote* custom endpoint is equally valid (custom shows no key field).
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "cloud",
            None,
            String::new(),
            String::from("https://my-proxy.example.com/v1"),
            String::from("some-model"),
            false,
            true,
        ),
        BackendResolution::Ready(_)
    ));
}

#[test]
fn resolve_cloud_without_an_endpoint_is_not_configured() {
    // Keyless provider but no base URL yet (e.g. custom before the user types one): there's
    // nothing to connect to, so it's genuinely not configured.
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "cloud",
            None,
            String::new(),
            String::new(),
            String::new(),
            false,
            true
        ),
        BackendResolution::NotConfigured(_)
    ));
}

// --- cloud AI consent: checked for cloud only, before any key or endpoint ---

fn cloud_ready_inputs(consented: bool) -> BackendResolution {
    resolve_backend_inner(
        &ManagedPolicy::default(),
        "cloud",
        None,
        String::from("sk-key"),
        String::from("https://api.openai.com/v1"),
        String::from("gpt-4o-mini"),
        true,
        consented,
    )
}

#[test]
fn cloud_without_consent_refuses_even_when_fully_configured() {
    assert!(matches!(cloud_ready_inputs(false), BackendResolution::NoCloudConsent));
}

#[test]
fn cloud_with_consent_is_ready() {
    assert!(matches!(cloud_ready_inputs(true), BackendResolution::Ready(_)));
}

/// Consent precedes setup: an unconfigured cloud provider without consent names the consent
/// gap, not the missing key.
#[test]
fn cloud_without_consent_or_key_names_the_consent_gap() {
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "cloud",
            None,
            String::new(),
            String::new(),
            String::new(),
            true,
            false
        ),
        BackendResolution::NoCloudConsent
    ));
}

/// Local AI never leaves the Mac, so it needs no consent.
#[test]
fn local_needs_no_cloud_consent() {
    assert!(matches!(
        resolve_backend_inner(
            &ManagedPolicy::default(),
            "local",
            Some(8080),
            String::new(),
            String::new(),
            String::new(),
            false,
            false
        ),
        BackendResolution::Ready(_)
    ));
}

#[test]
fn off_is_off_whatever_the_consent() {
    for consented in [true, false] {
        assert!(matches!(
            resolve_backend_inner(
                &ManagedPolicy::default(),
                "off",
                None,
                String::new(),
                String::new(),
                String::new(),
                true,
                consented
            ),
            BackendResolution::Off
        ));
    }
}

#[test]
fn a_missing_consent_maps_to_its_own_translate_kind() {
    let Err(err) = BackendResolution::NoCloudConsent.into_translate_result() else {
        panic!("a refused resolution must not translate to a backend");
    };
    assert_eq!(err.kind, AiTranslateErrorKind::NoCloudConsent);
}

#[test]
fn a_missing_consent_is_a_quiet_empty_for_nice_to_have_features() {
    assert!(BackendResolution::NoCloudConsent.ready_or_log("test").is_none());
}

#[test]
fn test_get_ai_status_no_manager() {
    // When manager is not initialized, status is Unavailable
    let status = get_ai_status();
    assert_eq!(status, AiStatus::Unavailable);
}

// --- compute_ai_status: pure decision function ---

const NOW: u64 = 1_700_000_000;

#[test]
fn compute_ai_status_provider_off_is_unavailable() {
    let s = compute_ai_status(&ManagedPolicy::default(), "off", true, true, None, true, NOW);
    assert_eq!(s, AiStatus::Unavailable);
}

#[test]
fn compute_ai_status_installed_and_running_is_available() {
    let s = compute_ai_status(&ManagedPolicy::default(), "local", true, true, None, true, NOW);
    assert_eq!(s, AiStatus::Available);
}

#[test]
fn compute_ai_status_installed_but_server_down_is_unavailable() {
    let s = compute_ai_status(&ManagedPolicy::default(), "local", true, false, None, true, NOW);
    assert_eq!(s, AiStatus::Unavailable);
}

#[test]
fn compute_ai_status_not_installed_offers_on_apple_silicon() {
    let s = compute_ai_status(&ManagedPolicy::default(), "local", false, false, None, true, NOW);
    assert_eq!(s, AiStatus::Offer);
}

#[test]
fn compute_ai_status_not_installed_does_not_offer_on_intel() {
    // The bug this guard fixes: Intel users with default provider="local" used to see
    // the AI download toast, only to be rejected by `start_ai_download` on click.
    let s = compute_ai_status(&ManagedPolicy::default(), "local", false, false, None, false, NOW);
    assert_eq!(s, AiStatus::Unavailable);
}

#[test]
fn compute_ai_status_intel_with_installed_state_still_unavailable() {
    // Defense in depth: even if state somehow says installed on Intel (e.g. user copied
    // their data dir across machines), we still don't claim Available because the binary
    // is ARM64-only and won't run.
    let s = compute_ai_status(&ManagedPolicy::default(), "local", true, false, None, false, NOW);
    assert_eq!(s, AiStatus::Unavailable);
}

#[test]
fn compute_ai_status_dismissed_offer_is_hidden() {
    let s = compute_ai_status(
        &ManagedPolicy::default(),
        "local",
        false,
        false,
        Some(NOW + 60),
        true,
        NOW,
    );
    assert_eq!(s, AiStatus::Unavailable);
}

#[test]
fn compute_ai_status_expired_dismissal_offers_again() {
    let s = compute_ai_status(
        &ManagedPolicy::default(),
        "local",
        false,
        false,
        Some(NOW - 60),
        true,
        NOW,
    );
    assert_eq!(s, AiStatus::Offer);
}
