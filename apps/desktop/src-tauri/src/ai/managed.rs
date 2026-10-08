//! The AI side of the organization's managed policy (`crate::managed_policy`): the local-model
//! gate, the immediate stops when a profile narrows AI while Cmdr runs, and the picker's batch
//! host check. The decisions themselves stay in `ManagedPolicy::ai_destination`; this module only
//! asks it.

use tauri::{AppHandle, Runtime};

use crate::managed_policy::{AiDestination, AiPolicy, ManagedAiRefusal, ManagedPolicy};

/// Whether Cmdr's own local model may download or run. Only `DisableAI` refuses it.
pub(super) fn local_ai_allowed(policy: &ManagedPolicy) -> Result<(), ManagedAiRefusal> {
    policy.ai_destination(&AiDestination::LocalServer)
}

/// What a policy change stops right away. The next request re-resolves and gets the typed reason;
/// this is for the work already running.
#[derive(Debug, Default, PartialEq, Eq)]
struct Stops {
    /// Cancel every in-flight Ask Cmdr turn and suggestion stream.
    in_flight_calls: bool,
    /// Stop `llama-server` and cancel a local model download.
    local_model: bool,
}

/// ANY narrowing of the AI policy stops every in-flight call, without working out which call talks
/// to which host: it's a rare event, and a call the new policy still allows simply runs again.
/// A change that leaves AI unrestricted stops nothing.
fn stops_for(old: &ManagedPolicy, new: &ManagedPolicy) -> Stops {
    let ai_changed = old.ai() != new.ai() || old.allowed_cloud_hosts() != new.allowed_cloud_hosts();
    let restricts_ai = new.ai() != AiPolicy::Allowed || new.allowed_cloud_hosts().is_some();
    Stops {
        in_flight_calls: ai_changed && restricts_ai,
        local_model: local_ai_allowed(new).is_err() && local_ai_allowed(old).is_ok(),
    }
}

/// Applies a policy change to AI: stops what [`stops_for`] says, then refreshes the wake loop's
/// cached readiness, whose answer the policy just moved. Called from `managed_policy`'s one change
/// handler.
pub fn apply_policy_change<R: Runtime>(app: &AppHandle<R>, old: &ManagedPolicy, new: &ManagedPolicy) {
    stop(&stops_for(old, new));
    crate::agent::wake::refresh_readiness(app);
}

fn stop(stops: &Stops) {
    if stops.in_flight_calls {
        super::cloud_consent::stop_in_flight_calls("the organization's AI policy narrowed");
    }
    if stops.local_model {
        log::info!("AI: the organization turned AI off; stopping the local server and any model download");
        super::server::stop_ai_server();
        super::install::cancel_ai_download();
    }
}

/// For each base URL, the policy's refusal of cloud AI sending there, or `None` when it may. The
/// provider picker renders a refused preset disabled with that reason, so the frontend never works
/// out which rule refused it. A URL, never a key, crosses IPC.
#[tauri::command]
#[specta::specta]
pub fn cloud_ai_host_verdicts(base_urls: Vec<String>) -> Vec<Option<ManagedAiRefusal>> {
    host_verdicts(&crate::managed_policy::current(), &base_urls)
}

fn host_verdicts(policy: &ManagedPolicy, base_urls: &[String]) -> Vec<Option<ManagedAiRefusal>> {
    base_urls
        .iter()
        .map(|url| policy.ai_destination(&super::client::remote_destination(url)).err())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::testing::{self, ALLOWED_CLOUD_AI_HOSTS, DISABLE_AI, DISABLE_CLOUD_AI};

    fn hosts(entries: &[&str]) -> ManagedPolicy {
        let list = entries.iter().map(|e| plist::Value::String((*e).to_string())).collect();
        testing::from_values(&[(ALLOWED_CLOUD_AI_HOSTS, plist::Value::Array(list))])
    }

    #[test]
    fn only_ai_off_refuses_the_local_model() {
        assert_eq!(local_ai_allowed(&ManagedPolicy::default()), Ok(()));
        assert_eq!(local_ai_allowed(&testing::forcing(&[DISABLE_CLOUD_AI])), Ok(()));
        assert_eq!(local_ai_allowed(&hosts(&[])), Ok(()));
        assert_eq!(
            local_ai_allowed(&testing::forcing(&[DISABLE_AI])),
            Err(ManagedAiRefusal::AiOff)
        );
    }

    #[test]
    fn narrowing_stops_calls_and_only_ai_off_stops_the_local_model() {
        let none = ManagedPolicy::default();
        let cloud_off = testing::forcing(&[DISABLE_CLOUD_AI]);
        let ai_off = testing::forcing(&[DISABLE_AI]);
        let calls = Stops {
            in_flight_calls: true,
            local_model: false,
        };
        let everything = Stops {
            in_flight_calls: true,
            local_model: true,
        };

        assert_eq!(stops_for(&none, &cloud_off), calls);
        assert_eq!(stops_for(&none, &hosts(&["api.openai.com"])), calls);
        assert_eq!(
            stops_for(&hosts(&["api.openai.com"]), &hosts(&["llm.corp.example"])),
            calls
        );
        assert_eq!(stops_for(&none, &ai_off), everything);
        assert_eq!(stops_for(&cloud_off, &ai_off), everything);
    }

    #[test]
    fn a_change_that_narrows_nothing_stops_nothing() {
        let none = ManagedPolicy::default();
        let cloud_off = testing::forcing(&[DISABLE_CLOUD_AI]);
        let ai_off = testing::forcing(&[DISABLE_AI]);
        assert_eq!(stops_for(&cloud_off, &none), Stops::default());
        assert_eq!(stops_for(&ai_off, &none), Stops::default());
        assert_eq!(stops_for(&none, &none), Stops::default());
        // A telemetry key moving leaves AI alone.
        let usage_off = testing::forcing(&[testing::DISABLE_USAGE_STATS]);
        assert_eq!(stops_for(&none, &usage_off), Stops::default());
        assert_eq!(stops_for(&ai_off, &ai_off), Stops::default());
    }

    /// The process-wide stream registry, so the id is deliberately unlike the app's UUIDs.
    #[test]
    fn ai_turning_off_cancels_registered_streams() {
        let token = super::super::stream_registry::register_stream("managed-policy-stop-test");
        stop(&stops_for(
            &ManagedPolicy::default(),
            &testing::forcing(&[DISABLE_CLOUD_AI]),
        ));
        assert!(token.is_cancelled());
        super::super::stream_registry::unregister_stream("managed-policy-stop-test");
    }

    #[test]
    fn the_picker_check_judges_each_url_as_the_backend_would() {
        let urls = vec![
            "https://api.openai.com/v1".to_string(),
            "https://my-tenant.openai.azure.com/openai/v1".to_string(),
            "http://localhost:11434/v1".to_string(),
            "not a url".to_string(),
        ];
        // Each URL carries the policy's own reason, so the picker never has to guess one.
        let not_listed = Some(ManagedAiRefusal::HostNotAllowed);
        assert_eq!(
            host_verdicts(&hosts(&["*.openai.azure.com"]), &urls),
            [not_listed, None, not_listed, not_listed]
        );
        let cloud_off = Some(ManagedAiRefusal::CloudAiOff);
        assert_eq!(
            host_verdicts(&testing::forcing(&[DISABLE_CLOUD_AI]), &urls),
            [cloud_off, cloud_off, cloud_off, cloud_off]
        );
        let ai_off = Some(ManagedAiRefusal::AiOff);
        assert_eq!(
            host_verdicts(&testing::forcing(&[DISABLE_AI]), &urls),
            [ai_off, ai_off, ai_off, ai_off]
        );
        assert_eq!(
            host_verdicts(&ManagedPolicy::default(), &urls),
            [None, None, None, None]
        );
    }
}
