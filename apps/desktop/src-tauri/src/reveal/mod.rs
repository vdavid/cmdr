//! "Reveal in Cmdr": another app's "Show in Finder" lands in our pane instead.
//!
//! Two halves. `registration` owns the one OS switch that makes it happen (the
//! `NSFileViewer` global default); `delivery` owns what arrives once it's on — macOS
//! delivers the reveal as an open-documents Apple Event, which Tauri surfaces as
//! `RunEvent::Opened`, handled in `app_lifecycle.rs`.
//!
//! This file itself carries only [`RevealDelivered`], because that type has to resolve on
//! every platform while all three submodules are macOS-only.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

// Every part that talks to macOS. This module itself holds only the event type, so
// [`RevealDelivered`] resolves on every platform for `ipc.rs`'s `collect_events!`,
// which can't cfg-gate inline.
#[cfg(target_os = "macos")]
pub mod commands;
#[cfg(target_os = "macos")]
pub mod delivery;
#[cfg(target_os = "macos")]
pub mod registration;

/// The one door every arriving reveal comes through, under its old name.
#[cfg(target_os = "macos")]
pub use delivery::on_urls_opened;

/// Typed `reveal-delivered` Tauri event: a reveal from another app just moved a pane.
///
/// Emitted only when the move actually landed, so it means "the feature just did its
/// thing", ❌ never "a reveal arrived". The frontend's one subscriber turns the FIRST of
/// these into a once-ever notice (`apps/desktop/src/lib/reveal/CLAUDE.md`), because cause
/// and effect here can be a week apart: someone switches this on, then an app they didn't
/// invoke jumps in front of them and nothing says why.
///
/// ❗ Payloadless on purpose. The paths are already on their way over `mcp-nav-to-path`,
/// and a second copy of them would be a second thing that can disagree.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct RevealDelivered {}
