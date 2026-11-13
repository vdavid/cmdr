//! The card's two commands. Pass-throughs: every decision is in `cmdr-about` and
//! the session.

use std::sync::Arc;

use tauri::ipc::Channel;

use super::event::{AboutEvent, AboutRequest, AboutUpdate};
use super::jobs::ROW_BUDGET;
use super::registry::REGISTRY;
use super::route::{VOLUME_BUDGET, gather_for};
use super::session::{AboutSink, GRACE, Submit, Timing, run};

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
/// `factsSettled` (or one `unavailable`) over `on_event`. Returns at once; the
/// gathering runs until it settles or `about_cancel` stops it.
///
/// ❗ Cancel with `about_cancel`, ❌ never by dropping the channel: a `Channel`'s
/// `send` still succeeds after the webview's handler is gone (`ai/CLAUDE.md`).
#[tauri::command]
pub async fn about_start(request: AboutRequest, on_event: Channel<AboutEvent>) {
    let Some(registered) = REGISTRY.register(&request.request_id) else {
        // The card closed before this start arrived.
        return;
    };
    let sink: Arc<dyn AboutSink> = Arc::new(ChannelSink {
        request_id: request.request_id.clone(),
        channel: on_event,
    });
    tauri::async_runtime::spawn(async move {
        let token = registered.token.clone();
        tokio::select! {
            () = token.cancelled() => {}
            gathered = gather_for(&request) => match gathered {
                Ok(gathered) => {
                    let submit: Submit = Arc::new(super::pool::submit);
                    let timing = Timing {
                        grace: GRACE,
                        row_budget: ROW_BUDGET,
                        volume_budget: VOLUME_BUDGET,
                    };
                    run(gathered, sink, token, submit, timing).await;
                }
                Err(why) => {
                    log::debug!(target: "about", "card unavailable: {why:?}");
                    let _ = sink.send(AboutUpdate::Unavailable { why });
                }
            },
        }
        REGISTRY.finish(&request.request_id, &registered);
    });
}

/// Stops the card's gathering: queued jobs skip their work, and nothing more is
/// sent. A cancel that arrives before its start makes that start a no-op.
#[tauri::command]
#[specta::specta]
pub fn about_cancel(request_id: String) {
    REGISTRY.cancel(&request_id);
}
