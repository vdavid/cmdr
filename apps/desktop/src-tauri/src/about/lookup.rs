//! A card gathered whole and handed over once, for the agent's `about_item`.
//!
//! ❌ No second gathering path: the same plan, jobs, budgets, and pool as the
//! card (`route::gather_for`, then `session::run`), with a sink that keeps the
//! card instead of streaming it. What leaves for the model goes through
//! `model::for_model` behind `model::gate_within`, so the tool can't skip the
//! sensitive gate either.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_about::card::AboutCard;
use cmdr_about::fact::FactKind;
use cmdr_about::model::ModelCard;
use tokio_util::sync::CancellationToken;

use super::event::{AboutRequest, AboutUpdate};
use super::model::{for_model, gate_within};
use super::registry::REGISTRY;
use super::route::{Asking, NoCard, Placed, Started, gather_for};
use super::session::{AboutSink, PROBE_BUDGET, Submit, Timing, run};
use crate::ignore_poison::IgnorePoison;
use crate::location::Location;
use crate::location::miss::{LocationMissResult, MissToken};

/// How long the lookup waits for the whole card. Past every card budget (the
/// probe's, then a row's or a volume's), so a row still out then is one whose
/// job never came back: it's named in [`LookedUp::still_out`], ❌ never dropped.
pub(crate) const LOOKUP_DEADLINE: Duration = Duration::from_secs(5);

/// The card as the agent reads it.
#[derive(Debug)]
pub(crate) struct LookedUp {
    pub(crate) location: Location,
    pub(crate) card: ModelCard,
    /// Every row the card has, in display order.
    pub(crate) rows: Vec<FactKind>,
    /// Rows that hadn't answered when the lookup stopped waiting.
    pub(crate) still_out: Vec<FactKind>,
    /// Cmdr started this, so the rows that read the item's volume weren't asked.
    pub(crate) refused_in_background: bool,
}

/// Looks up the item at `path` (on `volume_id`, else wherever the resolver
/// places it) until its card settles, [`LOOKUP_DEADLINE`] passes, or `cancel`
/// trips. A place that can't be described answers the location contract's miss.
pub(crate) async fn look_up(
    path: String,
    volume_id: Option<String>,
    started: Started,
    cancel: &CancellationToken,
) -> Result<LookedUp, Box<LocationMissResult>> {
    let submit: Submit = Arc::new(super::pool::submit);
    look_up_with(path, volume_id, started, cancel, submit, LOOKUP_DEADLINE).await
}

async fn look_up_with(
    path: String,
    volume_id: Option<String>,
    started: Started,
    cancel: &CancellationToken,
    submit: Submit,
    deadline: Duration,
) -> Result<LookedUp, Box<LocationMissResult>> {
    // Registered like a card, so quitting stops it too.
    let id = format!("about-item-{}", uuid::Uuid::new_v4());
    let request = AboutRequest {
        request_id: id.clone(),
        path,
        volume_id,
        is_directory: None,
    };
    let Some(registered) = REGISTRY.register(&id) else {
        return Err(stopped(&request));
    };
    let token = registered.token.clone();
    let answer = tokio::select! {
        // A stop that's already in wins over an answer that's ready too.
        biased;
        () = cancel.cancelled() => Err(stopped(&request)),
        () = token.cancelled() => Err(stopped(&request)),
        placed = gather_for(&request, started) => match placed {
            Err(NoCard { why, miss }) => {
                log::debug!(target: "about", "lookup unavailable: {why:?}");
                Err(miss)
            }
            Ok(Placed {
                gathered,
                location,
                asking,
                gate_reach,
            }) => {
                let kept = Arc::new(Kept::default());
                let sink: Arc<dyn AboutSink> = Arc::clone(&kept) as Arc<dyn AboutSink>;
                let gathering = async {
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => false,
                        () = run(gathered, sink, token.clone(), submit, Timing::CARD) => true,
                        () = tokio::time::sleep(deadline) => true,
                    }
                };
                let (finished, gated) = tokio::join!(
                    gathering,
                    gate_within(&location.path, gate_reach, PROBE_BUDGET)
                );
                // Stops the jobs still out and drops their late answers.
                token.cancel();
                match (finished, kept.take()) {
                    (true, Some(card)) => {
                        let still_out = card.pending();
                        log::debug!(
                            target: "about",
                            "lookup settled with {} row(s) still out",
                            still_out.len()
                        );
                        Ok(LookedUp {
                            card: for_model(&card, &location.path, gated),
                            rows: kept.rows(),
                            still_out,
                            refused_in_background: asking == Asking::RefusedInBackground,
                            location,
                        })
                    }
                    _ => Err(Box::new(LocationMissResult::on(
                        &location,
                        MissToken::Cancelled,
                        STOPPED.to_string(),
                    ))),
                }
            }
        },
    };
    REGISTRY.forget(&id, &registered);
    answer
}

const STOPPED: &str = "The lookup stopped before it finished. Ask again if it's still wanted.";

fn stopped(request: &AboutRequest) -> Box<LocationMissResult> {
    Box::new(LocationMissResult {
        miss: MissToken::Cancelled,
        location: crate::location::resolve::LocationInput {
            volume_id: request.volume_id.clone(),
            path: request.path.clone(),
        },
        detail: STOPPED.to_string(),
        root: None,
        nearby: None,
        same_share_as: None,
    })
}

/// A sink that keeps the card instead of streaming it.
#[derive(Default)]
struct Kept(Mutex<Option<KeptCard>>);

struct KeptCard {
    card: AboutCard,
    rows: Vec<FactKind>,
}

impl Kept {
    fn take(&self) -> Option<AboutCard> {
        self.0.lock_ignore_poison().as_ref().map(|kept| kept.card.clone())
    }

    fn rows(&self) -> Vec<FactKind> {
        self.0
            .lock_ignore_poison()
            .as_ref()
            .map(|kept| kept.rows.clone())
            .unwrap_or_default()
    }
}

impl AboutSink for Kept {
    fn send(&self, update: AboutUpdate) -> bool {
        let mut kept = self.0.lock_ignore_poison();
        match update {
            AboutUpdate::Planned { plan, .. } => {
                *kept = Some(KeptCard {
                    card: AboutCard::new(&plan),
                    rows: plan.kinds(),
                });
            }
            AboutUpdate::Fact { fact, .. } => {
                if let Some(kept) = kept.as_mut() {
                    // The session lands only planned rows, once each, so every answer applies.
                    let _ = kept.card.apply(fact);
                }
            }
            // The agent's lookup never asks for a verdict, so none of those arrive here.
            AboutUpdate::VolumeStoppedAnswering
            | AboutUpdate::FactsSettled
            | AboutUpdate::Unavailable { .. }
            | AboutUpdate::VerdictPlanned { .. }
            | AboutUpdate::VerdictUnavailable { .. }
            | AboutUpdate::VerdictGiven { .. }
            | AboutUpdate::VerdictDelta { .. }
            | AboutUpdate::VerdictEnd { .. }
            | AboutUpdate::VerdictFromEarlier { .. } => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// A row whose job never comes back by the deadline is named, never dropped
    /// and never waited on for good.
    #[tokio::test]
    async fn a_row_still_out_at_the_deadline_is_named() {
        crate::file_system::volume::manager::get_volume_manager().register_if_absent(
            "root",
            Arc::new(cmdr_local::LocalPosixVolume::local_folder(
                "Test root",
                "/",
                crate::volume_host::host(),
            )),
        );
        let dir = TestDir::new("about-lookup-deadline");
        let file = dir.as_ref().join("notes.txt");
        std::fs::write(&file, "x").expect("write the fixture");
        // A pool so busy nothing it's handed runs.
        let never: Submit = Arc::new(|_task| {});

        let looked_up = look_up_with(
            file.to_string_lossy().into_owned(),
            None,
            Started::ByPerson,
            &CancellationToken::new(),
            never,
            Duration::from_millis(50),
        )
        .await
        .expect("the card stands with what it has");

        assert!(!looked_up.still_out.is_empty(), "{looked_up:?}");
        assert!(
            !looked_up.still_out.contains(&FactKind::Identity),
            "the probe answered it"
        );
        let card = serde_json::to_value(&looked_up.card).expect("serializes");
        assert_eq!(card["identity"]["state"], "known");
        for kind in &looked_up.still_out {
            let row = serde_json::to_value(kind).expect("a token");
            let row = row.as_str().expect("a string");
            assert_eq!(card[row], serde_json::json!({ "state": "pending" }), "{row}");
        }
    }

    /// The turn's Stop ends the lookup at once, as a typed miss.
    #[tokio::test]
    async fn a_stop_ends_the_lookup() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let miss = look_up("/tmp".to_string(), None, Started::ByPerson, &cancel)
            .await
            .expect_err("stopped");
        assert_eq!(miss.miss, MissToken::Cancelled);
    }
}
