//! The card's commands. Pass-throughs: every decision is in `cmdr-about`, the
//! session, and the verdict.

use std::sync::Arc;

use tauri::AppHandle;
use tauri::ipc::Channel;

use super::event::{AboutEvent, AboutRequest, AboutUpdate, VerdictUnavailable};
use super::registry::REGISTRY;
use super::route::{Started, gather_for};
use super::session::{AboutSink, Submit, Timing, run};
use super::verdict::{self, VerdictRun, Watched};

/// A card's channel, stamping each update with its request id.
struct ChannelSink {
    request_id: String,
    channel: Channel<AboutEvent>,
}

impl AboutSink for ChannelSink {
    fn send(&self, update: AboutUpdate) -> bool {
        self.channel
            .send(AboutEvent {
                request_id: self.request_id.clone(),
                update,
            })
            .is_ok()
    }
}

/// Starts gathering the card for `request`, streaming `planned`, each fact, and
/// `factsSettled` (or one `unavailable`) over `on_event`, and beside them the
/// verdict's events. Returns at once; the gathering runs until it settles and the
/// verdict ends, or `about_cancel` stops both.
///
/// ❗ Cancel with `about_cancel`, ❌ never by dropping the channel: a `Channel`'s
/// `send` still succeeds after the webview's handler is gone (`ai/CLAUDE.md`).
#[tauri::command]
pub async fn about_start(app: AppHandle, request: AboutRequest, on_event: Channel<AboutEvent>) {
    let switch_app = app.clone();
    start(
        request,
        on_event,
        move || verdict::gate::resolve_for_card(&app),
        move || crate::settings::load_ask_cmdr_switch(&switch_app) == crate::settings::AskCmdrSwitch::On,
    );
}

/// [`about_start`] with the verdict's gate and Ask Cmdr's switch injected, so a test
/// runs it without an app.
pub(super) fn start(
    request: AboutRequest,
    on_event: Channel<AboutEvent>,
    gate: impl FnOnce() -> verdict::Decision + Send + 'static,
    ask_cmdr_on: impl FnOnce() -> bool + Send + 'static,
) {
    let Some(registered) = REGISTRY.register(&request.request_id) else {
        // The card closed before this start arrived.
        return;
    };
    let channel: Arc<dyn AboutSink> = Arc::new(ChannelSink {
        request_id: request.request_id.clone(),
        channel: on_event,
    });
    let watched = Arc::new(Watched::new(channel));
    tauri::async_runtime::spawn(async move {
        let token = registered.token.clone();
        // The verdict's gate (its setting, then the slot) resolves beside the card's
        // placing, so its answer is ready on the card's first frame. It reads
        // `settings.json` and `main.db`, so off the executor.
        let decision = tokio::task::spawn_blocking(gate);
        // Whether the footer offers "Continue in Ask Cmdr": read fresh per card, like the
        // send gate does, beside the verdict's gate.
        let ask_cmdr = tokio::task::spawn_blocking(ask_cmdr_on);
        tokio::select! {
            () = token.cancelled() => {}
            placed = gather_for(&request, Started::ByPerson) => match placed {
                Ok(mut placed) => {
                    let decision = decision.await.unwrap_or(Err(VerdictUnavailable::NotReady));
                    placed.gathered.actions.ask_cmdr = ask_cmdr.await.unwrap_or(false);
                    let verdict = VerdictRun {
                        request_id: request.request_id.clone(),
                        volume_id: placed.location.volume_id.clone(),
                        path: placed.location.path.clone(),
                        gate_reach: placed.asking.gate_reach(),
                        item: placed.gathered.item,
                        card: token.clone(),
                        locale: crate::intl::active_locale(),
                        limits: &verdict::limits::LIMITS,
                        read: verdict::peek::inspect_reader(),
                        facts_cap: verdict::FACTS_CAP,
                    };
                    let submit: Submit = Arc::new(super::pool::submit);
                    let sink: Arc<dyn AboutSink> = Arc::clone(&watched) as Arc<dyn AboutSink>;
                    let facts = async {
                        run(placed.gathered, sink, token.clone(), submit, Timing::CARD).await;
                        watched.finish();
                    };
                    tokio::join!(facts, verdict::run(verdict, decision, &watched));
                }
                Err(no_card) => {
                    let why = no_card.why;
                    log::debug!(target: "about", "card unavailable: {why:?}");
                    let _ = AboutSink::send(watched.as_ref(), AboutUpdate::Unavailable { why });
                }
            },
        }
        REGISTRY.finish(&request.request_id, &registered);
    });
}

/// Stops the card's gathering and its verdict: queued jobs skip their work, and
/// nothing more is sent. A cancel that arrives before its start makes that start a
/// no-op.
#[tauri::command]
#[specta::specta]
pub fn about_cancel(request_id: String) {
    REGISTRY.cancel(&request_id);
}

/// Stops the card's verdict alone and leaves the card open: its facts are done, and
/// the text so far stands with a `stopped` ending.
#[tauri::command]
#[specta::specta]
pub fn about_stop_verdict(request_id: String) {
    if !verdict::limits::LIMITS.stop(&request_id) {
        log::debug!(target: "about", "no verdict in flight for that card to stop");
    }
}
