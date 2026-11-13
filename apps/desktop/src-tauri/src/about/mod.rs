//! "Tell me about this": the host that gathers a card's facts.
//!
//! `cmdr-about` owns the vocabulary and every rule; this module reads the raw
//! inputs (an `lstat`, a file's first bytes, an xattr, Spotlight, the code
//! signature) and streams the card over a channel. Docs: `CLAUDE.md`, `DETAILS.md`.

pub(crate) mod commands;
mod event;
mod history;
mod jobs;
mod platform;
mod pool;
mod probe;
mod registry;
mod route;
mod session;

#[cfg(test)]
mod tests;

pub use event::AboutEvent;

/// Stops every card's gathering: the app is quitting.
pub(crate) fn cancel_all() {
    let stopped = registry::REGISTRY.cancel_all();
    if stopped > 0 {
        log::debug!(target: "about", "stopped {stopped} card(s) on quit");
    }
}
