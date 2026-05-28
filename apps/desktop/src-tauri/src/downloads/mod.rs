//! `~/Downloads` watcher.
//!
//! Recursive `notify` watch on the resolved Downloads dir, tied to the FDA
//! gate. Emits `download-detected` Tauri events for eligible final-form
//! files, suppressing events whose paths Cmdr's own writes registered in
//! advance via [`IgnoreSet`].
//!
//! ## Pieces
//!
//! - [`is_eligible`] decides whether an observed path looks like a "real"
//!   download (not hidden, not partial, regular file).
//! - [`IgnoreSet`] holds paths Cmdr just wrote, with a 5 s TTL, so the
//!   watcher can drop its own events. Bounded with FIFO eviction at 1000
//!   entries.
//! - [`LatestRing`] keeps the last ~10 observed downloads in insertion
//!   order so [`reveal_latest_download`] returns the latest without a
//!   directory scan.
//! - [`DownloadsWatcher`] glues these to a `notify-debouncer-full` handle
//!   and an `EventSink` (production: `AppHandle::emit`; tests: mpsc).
//! - [`commands`] exposes the IPC surface (`reveal_latest_download`,
//!   `downloads_watcher_status`, `recheck_downloads_watcher_gate`).
//! - [`runtime`] holds the process-global watcher handle. `lib.rs` calls
//!   [`refresh_runtime`] at startup and on focus events.
//!
//! ## FDA gating contract
//!
//! The watcher is alive iff `crate::fda_gate::is_fda_pending_runtime() ==
//! false`. `lib.rs` enforces this at startup and on every main-window
//! `Focused(true)` event; the Settings pane re-checks on mount as a
//! belt-and-braces. Logic lives in [`desired_running`].
//!
//! ## Cmdr-own-write ignore set (M3 hook contract)
//!
//! Write operations call [`DownloadsWatcher::note_pending_write`] (or
//! `note_pending_writes` for batches) just before issuing the syscall. The
//! default TTL is [`watcher::DEFAULT_IGNORE_TTL`] (5 s). Call sites can
//! invoke unconditionally: the ignore set silently no-ops for paths
//! outside the resolved Downloads root (locked-in scoping; don't move the
//! filter to the call sites). Key on the **final** path, not the partial —
//! browser rename `foo.zip.crdownload` → `foo.zip` arrives as
//! `RenameMode::Both` carrying both paths, and the watcher checks both
//! halves against the ignore set.
//!
//! ## Browser-style rename target
//!
//! v1 explicitly scopes to browser-style downloads that finalize via a
//! rename from a partial-suffix path (`.crdownload`, `.part`, `.download`)
//! to a final-name file, or a direct create of a final-name file. CLI
//! tools that write directly to the final name with no rename signal
//! (curl/wget, `cp` from Terminal, 7-Zip extracting) are out of scope.
//! See `docs/specs/downloads-watcher-plan.md` § "Latest download
//! definition" for the rationale.
//!
//! ## Gotchas
//!
//! - **No `tokio::spawn` from the notify callback.** The debouncer
//!   callback runs on `notify-rs`'s internal thread with no Tokio runtime
//!   context. Synchronous work (`is_eligible`, ring push, sink emit) stays
//!   inline; if async work is ever needed here, use
//!   `tauri::async_runtime::spawn`. Matches the listing watcher's pattern
//!   (`file_system::watcher`).
//! - **No `println!` / `eprintln!` / `dbg!`.** Clippy denies these
//!   crate-wide. Use `log::debug!` / `log::info!` / `log::warn!` with
//!   `target: "downloads::watcher"`.

pub mod commands;
mod filter;
mod ignore_set;
mod latest_ring;
pub mod runtime;
pub mod watcher;

pub use filter::is_eligible;
pub use ignore_set::IgnoreSet;
pub use latest_ring::LatestRing;
pub use runtime::{note_pending_write_for_cmdr, refresh_runtime};
pub use watcher::{DownloadsWatcher, WatcherError, desired_running};
