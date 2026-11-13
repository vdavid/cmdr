//! The card's channel as the verdict sees it: every fact update passes through to the
//! card unchanged, and on its way the verdict learns what it needs (the card as it
//! stands, and whether `planned` and `factsSettled` went out).

use std::sync::{Arc, Mutex};

use cmdr_about::card::AboutCard;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::super::event::AboutUpdate;
use super::super::session::AboutSink;
use crate::ignore_poison::IgnorePoison;

/// How far the card's facts have got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Phase {
    /// Nothing sent yet.
    Waiting,
    /// `planned` went out.
    Planned,
    /// `factsSettled` went out.
    Settled,
    /// The card went `unavailable`, or its gathering ended without settling: nothing
    /// more is coming.
    Gone,
}

/// A sink in front of the card's channel.
pub(crate) struct Watched {
    inner: Arc<dyn AboutSink>,
    card: Mutex<Option<AboutCard>>,
    phase: watch::Sender<Phase>,
}

impl Watched {
    pub(crate) fn new(inner: Arc<dyn AboutSink>) -> Self {
        Self {
            inner,
            card: Mutex::new(None),
            phase: watch::Sender::new(Phase::Waiting),
        }
    }

    /// Waits until the facts reach `phase`. `false` when they never will (the card is
    /// gone) or `cancel` tripped first.
    pub(crate) async fn wait_for(&self, phase: Phase, cancel: &CancellationToken) -> bool {
        let mut seen = self.phase.subscribe();
        tokio::select! {
            biased;
            () = cancel.cancelled() => false,
            reached = seen.wait_for(|now| *now >= phase) => {
                matches!(reached.as_deref(), Ok(now) if *now != Phase::Gone || phase == Phase::Gone)
            }
        }
    }

    /// Nothing more about the facts is coming (their gathering returned). Lets a
    /// verdict still waiting for the settle go with what's in.
    pub(crate) fn finish(&self) {
        self.phase.send_if_modified(|now| {
            if *now == Phase::Waiting {
                *now = Phase::Gone;
                true
            } else if *now == Phase::Planned {
                // The gathering returned without settling (a cancel): treat as settled,
                // so a waiting verdict isn't held to its cap for nothing.
                *now = Phase::Settled;
                true
            } else {
                false
            }
        });
    }

    /// The card as it stands. Empty before `planned`.
    pub(crate) fn card(&self) -> AboutCard {
        self.card.lock_ignore_poison().clone().unwrap_or_else(|| {
            AboutCard::new(&cmdr_about::card::AboutPlan {
                target: cmdr_about::fact::AboutTarget::File,
                sections: Vec::new(),
                volume_note: None,
            })
        })
    }

    /// Sends one of the verdict's own updates, unless the card is closed.
    pub(crate) fn send_verdict(&self, card: &CancellationToken, update: AboutUpdate) {
        if card.is_cancelled() {
            return;
        }
        if !self.inner.send(update) {
            card.cancel();
        }
    }
}

impl AboutSink for Watched {
    fn send(&self, update: AboutUpdate) -> bool {
        let next = {
            let mut card = self.card.lock_ignore_poison();
            match &update {
                AboutUpdate::Planned { plan, .. } => {
                    *card = Some(AboutCard::new(plan));
                    Some(Phase::Planned)
                }
                AboutUpdate::Fact { fact, .. } => {
                    if let Some(card) = card.as_mut() {
                        // The session lands only planned rows, once each.
                        let _ = card.apply(fact.clone());
                    }
                    None
                }
                AboutUpdate::FactsSettled => Some(Phase::Settled),
                AboutUpdate::Unavailable { .. } => Some(Phase::Gone),
                _ => None,
            }
        };
        // Onto the channel first, so the verdict's word after `planned` lands after it.
        let delivered = self.inner.send(update);
        if let Some(next) = next {
            self.phase.send_replace(next);
        }
        delivered
    }
}
