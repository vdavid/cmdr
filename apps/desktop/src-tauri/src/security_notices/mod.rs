//! Telling the person, in the moment, that Cmdr turned something away: a web page reaching for
//! the MCP server, a server reply refused for its signature, an update that isn't a genuine
//! Cmdr release. The CRA's Annex I Part I (2)(d) asks to "report on possible unauthorised
//! access"; the security log (`crate::security_log`) is the record, this is the notice.
//!
//! One typed event, one emitter: this module is a `security_log::subscribe`r, so every source
//! keeps calling `security_log::record` (or a `note_*_rejection`) and nothing else. Which events
//! reach the person is [`notice_for`]'s call; how often is [`gate::NoticeGate`]'s. The frontend
//! words each notice (`apps/desktop/src/lib/security-notices/`). Design: `DETAILS.md`.

mod gate;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::ignore_poison::IgnorePoison;
use crate::security_log::{self, Heard, McpRejection, SecurityEvent, SignatureCheck};

const LOG_TARGET: &str = "security_notices";

/// At most one re-raise per notice per window, so a client retrying in a loop updates one toast
/// once a minute rather than flooding the screen.
const WINDOW: Duration = Duration::from_secs(60);

/// Something turned away that the person hears about. Carries its source, so it's also the
/// gate's key, and ❌ never anything the person shouldn't see in a toast (no origin, no path).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AccessNotice {
    /// A web page in a browser tried to reach the MCP server (a non-loopback `Origin`). The
    /// origin itself is in the log, never here: it's a site the person visits.
    McpForeignOrigin,
    /// A server reply on a signed session carried a bad or missing signature.
    SmbSignatureRefused { volume_id: String },
    /// An offered update didn't carry Cmdr's signature, so nothing was installed.
    UpdateNotGenuine { version: String, check: SignatureCheck },
}

/// Typed `security-notice-raised` event: tell the person about `notice`, seen `count` times
/// since Cmdr started. A later raise for the same notice carries the grown count, so the
/// frontend updates its toast in place.
///
/// `Serialize` only beside the derive, like `AgentWakeStaged`: nothing listens Rust-side.
#[derive(Debug, Clone, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "security-notice-raised")]
pub struct SecurityNoticeRaised {
    pub notice: AccessNotice,
    pub count: u64,
}

/// Which events the person hears about. Everything else is the record's alone: a missing or
/// wrong MCP token (a client being set up, or one holding the previous run's token, David's
/// call 2026-10-16), a policy refusal, an install, an episode's end, a recovered state file.
fn notice_for(event: &SecurityEvent) -> Option<AccessNotice> {
    match event {
        SecurityEvent::McpRejectionsStarted {
            reason: McpRejection::ForeignOrigin,
        } => Some(AccessNotice::McpForeignOrigin),
        SecurityEvent::SmbSignatureRejectionsStarted { volume_id } => Some(AccessNotice::SmbSignatureRefused {
            volume_id: volume_id.clone(),
        }),
        SecurityEvent::UpdateSignatureInvalid { version, check } => Some(AccessNotice::UpdateNotGenuine {
            version: version.clone(),
            check: *check,
        }),
        SecurityEvent::McpRejectionsStarted {
            reason: McpRejection::MissingToken | McpRejection::WrongToken | McpRejection::NoTokenConfigured,
        }
        | SecurityEvent::McpRejectionsEnded { .. }
        | SecurityEvent::SmbSignatureRejectionsEnded { .. }
        | SecurityEvent::CloudAiConsentGiven
        | SecurityEvent::CloudAiConsentWithdrawn
        | SecurityEvent::McpServerTurnedOn { .. }
        | SecurityEvent::McpServerTurnedOff { .. }
        | SecurityEvent::ManagedPolicyApplied { .. }
        | SecurityEvent::UpdateInstalled { .. }
        | SecurityEvent::UpdateRefusedByPolicy { .. }
        | SecurityEvent::RecordingTurnedOn { .. }
        | SecurityEvent::RecordingTurnedOff { .. }
        // The recovery notice has its own channel, since it names where the copy went:
        // `crate::state_recovery`.
        | SecurityEvent::StateFileRecovered { .. }
        // The tool result already tells the person, on the rail; a notice would only repeat it.
        | SecurityEvent::ContentRefusalsStarted { .. }
        | SecurityEvent::ContentRefusalsEnded { .. } => None,
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static GATE: LazyLock<Mutex<gate::NoticeGate>> = LazyLock::new(|| Mutex::new(gate::NoticeGate::new(WINDOW)));
static FLUSH_SCHEDULED: AtomicBool = AtomicBool::new(false);

/// Starts listening to the security log. Call once from `setup()`, after `security_log::init`.
pub fn init(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    security_log::subscribe(Arc::new(on_event));
}

/// The subscriber. A record and a repeat count alike: each is one more refusal for the gate.
fn on_event(event: &SecurityEvent, _heard: Heard) {
    let Some(notice) = notice_for(event) else {
        return;
    };
    let raise = GATE.lock_ignore_poison().note(notice, Instant::now());
    match raise {
        Some(raise) => emit(raise),
        None => schedule_flush(),
    }
}

fn emit(raise: gate::Raise) {
    let Some(app) = APP.get() else {
        return;
    };
    let event = SecurityNoticeRaised {
        notice: raise.notice,
        count: raise.count,
    };
    if let Err(e) = event.emit(app) {
        log::warn!(target: LOG_TARGET, "A security notice didn't reach the windows: {e}");
    }
}

/// Raises the held counts once their window ends, on a short-lived thread that exits when
/// nothing is held, so an idle app runs nothing for this.
fn schedule_flush() {
    if FLUSH_SCHEDULED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("security-notices-flush".into())
        .spawn(|| {
            loop {
                let next = GATE.lock_ignore_poison().next_due();
                let Some(next) = next else {
                    FLUSH_SCHEDULED.store(false, Ordering::SeqCst);
                    // A refusal between the read and the store saw the flag set and didn't
                    // schedule, so keep going when one is held now.
                    if GATE.lock_ignore_poison().next_due().is_none() || FLUSH_SCHEDULED.swap(true, Ordering::SeqCst) {
                        return;
                    }
                    continue;
                };
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
                let due = GATE.lock_ignore_poison().due(Instant::now());
                for raise in due {
                    emit(raise);
                }
            }
        });
    if let Err(e) = spawned {
        FLUSH_SCHEDULED.store(false, Ordering::SeqCst);
        log::warn!(target: LOG_TARGET, "The notice flusher didn't start, so a held count waits for the next refusal: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_web_page_a_refused_reply_and_a_fake_update_reach_the_person() {
        assert_eq!(
            notice_for(&SecurityEvent::McpRejectionsStarted {
                reason: McpRejection::ForeignOrigin
            }),
            Some(AccessNotice::McpForeignOrigin)
        );
        assert_eq!(
            notice_for(&SecurityEvent::SmbSignatureRejectionsStarted {
                volume_id: "smb-nas".into()
            }),
            Some(AccessNotice::SmbSignatureRefused {
                volume_id: "smb-nas".into()
            })
        );
        for check in [SignatureCheck::Minisign, SignatureCheck::CodeSignature] {
            assert_eq!(
                notice_for(&SecurityEvent::UpdateSignatureInvalid {
                    version: "0.53.0".into(),
                    check
                }),
                Some(AccessNotice::UpdateNotGenuine {
                    version: "0.53.0".into(),
                    check
                })
            );
        }
    }

    /// A missing or wrong MCP token is the record's alone (cases 1 and 2 of #457), and so is
    /// everything that isn't a refusal, or is an episode's end.
    #[test]
    fn tokens_and_everything_else_stay_in_the_record() {
        for reason in [
            McpRejection::MissingToken,
            McpRejection::WrongToken,
            McpRejection::NoTokenConfigured,
        ] {
            assert_eq!(notice_for(&SecurityEvent::McpRejectionsStarted { reason }), None);
        }
        for event in [
            SecurityEvent::McpRejectionsEnded {
                reason: McpRejection::ForeignOrigin,
                count: 3,
            },
            SecurityEvent::SmbSignatureRejectionsEnded {
                volume_id: "smb-nas".into(),
                count: 3,
            },
            SecurityEvent::UpdateInstalled {
                from_version: "0.52.0".into(),
                to_version: "0.53.0".into(),
            },
            SecurityEvent::CloudAiConsentGiven,
        ] {
            assert_eq!(notice_for(&event), None, "{event:?}");
        }
    }
}
