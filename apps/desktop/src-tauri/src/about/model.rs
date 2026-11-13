//! The card as a model may read it: the host's half of `cmdr-about`'s one
//! projection (`AboutCard::for_model`).
//!
//! Both senders (the agent tool and the verdict) call [`for_model`] here, never
//! the crate's method: this is where the path's sensitive-content answer and the
//! app's own size and date words go in, so neither sender can forget the gate or
//! word a number its own way.

use cmdr_about::card::AboutCard;
use cmdr_about::model::{ModelCard, ProtectedReason, Speak, SpokenSize};

use crate::tools::sensitive::{self, Protected};

/// What may leave the machine about the item at `path`. Sync and pure: the
/// sensitive gate's `classify` reads path components only.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the agent tool and the verdict are its callers, and neither exists yet"
    )
)]
pub(crate) fn for_model(card: &AboutCard, path: &str) -> ModelCard {
    card.for_model(path, sensitive::classify(path).map(reason), &AppWords)
}

fn reason(protected: Protected) -> ProtectedReason {
    match protected {
        Protected::Credentials => ProtectedReason::Credentials,
        Protected::BrowserData => ProtectedReason::BrowserData,
        Protected::MessageStore => ProtectedReason::MessageStore,
        Protected::CmdrData => ProtectedReason::CmdrData,
        Protected::PrivateKey => ProtectedReason::PrivateKey,
    }
}

/// The words every other tool result uses for a size and a date.
struct AppWords;

impl Speak for AppWords {
    fn size(&self, size: SpokenSize) -> String {
        crate::tools::read::listing::human_size(size.bytes, size.lower_bound, size.updating)
    }

    fn date(&self, secs: i64) -> String {
        crate::search::format_local_date(secs.max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use cmdr_about::card::AboutCard;
    use cmdr_about::fact::{AboutTarget, Fact};
    use cmdr_about::payload::Identity;
    use cmdr_about::text::ItemText;

    use super::*;

    fn card() -> AboutCard {
        AboutCard {
            target: AboutTarget::File,
            identity: Some(Fact::Known(Identity {
                name: ItemText::new("id_ed25519"),
                logical_size: Some(1_500_000),
                physical_size: None,
                created: None,
                modified: Some(1_772_400_000),
                hard_links: Some(1),
                symlink_target: None,
            })),
            real_type: Some(Fact::Pending),
            bundle: None,
            signature: None,
            download: None,
            cmdr_operation: None,
            last_opened: None,
            default_app: None,
            importance: None,
            owner: None,
            git: None,
            recursive_size: None,
            folder_digest: None,
            app_data: None,
            git_snapshot: None,
        }
    }

    /// The host answers the gate itself, so a sender can't hand a credentials
    /// folder's item to a model with its recorded facts.
    #[test]
    fn a_path_the_gate_protects_leaves_name_level_facts_only() {
        let leaving = serde_json::to_value(for_model(&card(), "/Users/jane/.ssh/id_ed25519")).expect("serializes");
        assert_eq!(leaving["contentsStayHome"], "credentials");
        assert_eq!(
            leaving["realType"],
            serde_json::json!({ "state": "unknown", "value": "protected" })
        );
        assert_eq!(leaving["identity"]["value"]["name"], "id_ed25519");

        let open = serde_json::to_value(for_model(&card(), "/Users/jane/Documents/id_ed25519")).expect("serializes");
        assert!(open.get("contentsStayHome").is_none());
        assert_eq!(open["realType"], serde_json::json!({ "state": "pending" }));
    }

    /// Sizes and dates read as every other tool result words them.
    #[test]
    fn numbers_arrive_in_the_apps_own_words() {
        let leaving = serde_json::to_value(for_model(&card(), "/Users/jane/Documents/id_ed25519")).expect("serializes");
        let identity = &leaving["identity"]["value"];
        assert_eq!(
            identity["logicalSizeHuman"],
            crate::tools::read::listing::human_size(1_500_000, false, false)
        );
        assert_eq!(
            identity["modifiedHuman"],
            crate::search::format_local_date(1_772_400_000)
        );
        assert!(
            identity.get("physicalSizeHuman").is_none(),
            "an unknown size gets no words"
        );
    }
}
