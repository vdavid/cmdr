//! FSEvents-under-TCC probe for the downloads watcher.
//!
//! Pre-flight diagnostic for the downloads watcher (`src/downloads/`). This
//! probe answers the open question: does `notify::recommended_watcher`
//! deliver events for `~/Downloads` when the user has only granted the
//! per-folder Downloads TCC consent, or does it require full Full Disk Access?
//!
//! This is a one-shot diagnostic, not part of the running app. `println!` is
//! the natural idiom here, hence the cargo example (where it's allowed) rather
//! than a module under `src/` (where clippy denies `println!` crate-wide; see
//! `src-tauri/src/logging/CLAUDE.md`).
//!
//! # How to run
//!
//! From `apps/desktop/src-tauri/`:
//!
//! ```sh
//! cargo run --example probe_downloads_fsevents
//! ```
//!
//! The probe watches `~/Downloads` recursively via `notify::recommended_watcher`
//! (FSEvents on macOS, the same backend the real watcher will use). It prints
//! one line per event with a timestamp, the event kind, and the affected paths.
//! It runs until Ctrl-C or 60 seconds, whichever comes first.
//!
//! While it's running, drop a file into `~/Downloads` (a real browser
//! download, a Finder copy, a `cp` from Terminal — any of these). Each
//! observed event should print.
//!
//! # What to test
//!
//! Run the probe **twice**, with the OS in two different TCC states:
//!
//! 1. **Full Disk Access granted to the parent process** (Terminal, iTerm,
//!    whichever shell you launch `cargo` from). Baseline: events should
//!    arrive. If they don't, something's wrong with the probe itself, not
//!    with TCC.
//!
//! 2. **FDA revoked, per-folder Downloads consent only.** To get here:
//!    - Open System Settings > Privacy & Security > Full Disk Access.
//!    - Toggle off whichever process you're running the probe from (Terminal,
//!      iTerm, etc.). macOS will ask you to quit and reopen it; do that so
//!      the change takes effect.
//!    - Run the probe again. macOS should pop the per-folder Downloads
//!      consent dialog on the first read attempt — accept it.
//!    - Drop files into `~/Downloads` and watch for events.
//!
//! # What to look for
//!
//! - Do events arrive in **both** modes, or only with full FDA?
//! - If per-folder consent is enough, the Settings UX can offer the lighter
//!   prompt as a path forward, and the FDA-gating copy needs to mention the
//!   per-folder option.
//! - If full FDA is required, the Settings hint copy has to say so, and the
//!   feature stays gated on `is_fda_pending_runtime()` as planned.
//!
//! Report findings back so risk 1 in the plan ("`notify` on macOS Downloads
//! under TCC") can be resolved and the FDA-gating UX finalized.
//!
//! # Notes
//!
//! - 60-second timeout is a hard cap; press Ctrl-C earlier when you've seen
//!   enough.
//! - The probe doesn't filter events at all — even spurious
//!   `Access(Close(Write))` and metadata events print. The real watcher will
//!   filter; this is intentionally raw so we see exactly what FSEvents emits.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use chrono::Local;
use notify::{Event, RecursiveMode, Watcher, recommended_watcher};

const PROBE_DURATION: Duration = Duration::from_secs(60);

fn main() {
    let downloads = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .expect("Could not resolve ~/Downloads");

    println!("Probe: watching {} recursively", downloads.display());
    println!(
        "Probe: runs for up to {} s, Ctrl-C to stop early",
        PROBE_DURATION.as_secs()
    );
    println!("Probe: drop files into Downloads to see events");
    println!();

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher = recommended_watcher(move |res| {
        // If the receiver is gone (main exited), drop the send error silently.
        let _ = tx.send(res);
    })
    .expect("Failed to create notify watcher");

    watcher
        .watch(&downloads, RecursiveMode::Recursive)
        .expect("Failed to start watching Downloads");

    let started = Instant::now();
    let mut event_count: usize = 0;

    loop {
        let elapsed = started.elapsed();
        if elapsed >= PROBE_DURATION {
            println!();
            println!("Probe: time limit reached after {} s", PROBE_DURATION.as_secs());
            break;
        }
        let remaining = PROBE_DURATION - elapsed;
        // Poll with a short timeout so we honour the overall deadline even
        // when no events arrive.
        match rx.recv_timeout(remaining.min(Duration::from_millis(500))) {
            Ok(Ok(event)) => {
                event_count += 1;
                let ts = Local::now().format("%H:%M:%S%.3f");
                let paths: Vec<PathBuf> = event.paths.clone();
                println!("[{ts}] kind={:?} paths={:?}", event.kind, paths);
            }
            Ok(Err(err)) => {
                let ts = Local::now().format("%H:%M:%S%.3f");
                println!("[{ts}] watcher error: {err}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Loop again and re-check the overall deadline.
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("Probe: watcher channel closed unexpectedly");
                break;
            }
        }
    }

    println!("Probe: observed {event_count} event(s) total");
}
