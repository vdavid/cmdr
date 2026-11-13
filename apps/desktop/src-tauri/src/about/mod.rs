//! "Tell me about this": the host that gathers a card's facts.
//!
//! `cmdr-about` owns the vocabulary and every rule; this module reads the raw
//! inputs (an `lstat`, a file's first bytes, an xattr, Spotlight, the code
//! signature) and streams the card over a channel. Docs: `CLAUDE.md`, `DETAILS.md`.

pub(crate) mod commands;
mod digest;
mod event;
mod git;
mod history;
mod index;
mod jobs;
pub(crate) mod lookup;
mod model;
mod owner;
mod platform;
mod pool;
mod probe;
mod registry;
mod route;
mod session;
mod through;
pub(crate) mod verdict;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod through_tests;

pub use event::AboutEvent;
pub(crate) use route::Started;

/// A card's projection for a test outside this module, through the one door.
#[cfg(test)]
pub(crate) fn model_for_test(card: &cmdr_about::card::AboutCard, path: &str) -> cmdr_about::model::ModelCard {
    model::for_model(card, path, model::gate(path, model::GateReach::NamesOnly))
}

/// Stops the card verdict in flight and leaves its card open: cloud AI turned off, or the
/// organization's policy narrowed (`ai::cloud_consent::stop_in_flight_calls`). Returns how
/// many it stopped.
pub(crate) fn stop_verdicts() -> usize {
    verdict::stop_all()
}

/// Stops every card's gathering, verdicts included: the app is quitting.
pub(crate) fn cancel_all() {
    let stopped = registry::REGISTRY.cancel_all();
    if stopped > 0 {
        log::debug!(target: "about", "stopped {stopped} card(s) on quit");
    }
}
