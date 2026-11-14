//! The card as a model may read it: the host's half of `cmdr-about`'s one
//! projection (`model::for_model`).
//!
//! Both senders (the agent tool and the verdict) call [`for_model`] here, never
//! the crate's own function, and [`for_model`] takes only a [`Gate`] that [`gate`]
//! answered: this is where the sensitive-content gate and the app's own size and
//! date words go in, so neither sender can skip the gate or word a number its own
//! way.

use std::time::Duration;

use cmdr_about::card::AboutCard;
use cmdr_about::model::{ModelCard, ProtectedReason, Speak, SpokenSize};

use crate::tools::sensitive::{self, GateStop, Protected};

/// How far the gate may follow a path before it answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateReach {
    /// `inspect_file`'s rules on the path as given and again after following its
    /// symlinks, so `~/link-to-ssh/key` is caught, with every link followed by
    /// `place`'s walk: on local disks only, ❌ never into a share (past a share's
    /// edge the rest stays as written). For a card on a local disk.
    FollowLocalLinks,
    /// The rules and Cmdr's data dir on the path as given, with no call to the
    /// volume (`sensitive::names_gate`): a card reached through its volume (a
    /// share, a server, a phone), and a card Cmdr asked for on its own whose rows
    /// that read the volume answer `RefusedInBackground`. Nothing on such a card
    /// reads the item's contents, and the index rows store the paths they walked,
    /// so the path as given is the path those rows describe (the precedent
    /// `image_facts` sets).
    NamesOnly,
}

/// The sensitive gate's answer for one item. Only [`gate`] makes one, so a
/// projection can't be built on a path nobody checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Gate(Option<ProtectedReason>);

impl Gate {
    /// Why the item's contents stay home, if they do.
    pub(crate) fn protected(self) -> Option<ProtectedReason> {
        self.0
    }

    /// The sensitive gate's own reason, for the security log's refusal line. `None` when
    /// the item isn't protected, or when the gate couldn't finish (failing closed isn't a
    /// rule saying no, so there's no rule to log).
    pub(crate) fn refusal(self) -> Option<Protected> {
        match self.0? {
            ProtectedReason::Credentials => Some(Protected::Credentials),
            ProtectedReason::BrowserData => Some(Protected::BrowserData),
            ProtectedReason::MessageStore => Some(Protected::MessageStore),
            ProtectedReason::CmdrData => Some(Protected::CmdrData),
            ProtectedReason::PrivateKey => Some(Protected::PrivateKey),
            ProtectedReason::Unresolved => None,
        }
    }
}

/// Checks `path` for a model. Blocking with [`GateReach::FollowLocalLinks`] (the
/// walk `lstat`s each local ancestor): run it where a deadline can abandon it, as
/// [`gate_within`] does.
pub(crate) fn gate(path: &str, reach: GateReach) -> Gate {
    let answer = match reach {
        GateReach::NamesOnly => sensitive::names_gate(path),
        GateReach::FollowLocalLinks => match super::place::resolved_on_local_disks(path) {
            Some(real) => sensitive::resolved_gate(path, &real),
            // Nothing proves where the path goes (a link loop, an unreadable mount
            // table): fail closed, with the rule's own reason when the name has one.
            None => {
                return Gate(Some(
                    sensitive::names_gate(path)
                        .err()
                        .map_or(ProtectedReason::Unresolved, reason),
                ));
            }
        },
    };
    Gate(answer.err().map(reason))
}

/// [`gate`] under a deadline, failing closed: a resolve still out at `budget` (a
/// local disk that stopped answering) answers [`ProtectedReason::Unresolved`], so
/// the model gets name-level facts only. The resolve runs on a blocking thread,
/// ❌ never the about pool: queued behind the card's own slow jobs, it would time
/// out on a healthy disk.
pub(crate) async fn gate_within(path: &str, reach: GateReach, budget: Duration) -> Gate {
    match reach {
        GateReach::NamesOnly => gate(path, reach),
        GateReach::FollowLocalLinks => {
            // The rules on the name need no thread, and a protected name stays
            // protected whether or not the resolve finishes.
            if let Err(protected) = sensitive::names_gate(path) {
                return Gate(Some(reason(protected)));
            }
            let owned = path.to_string();
            let resolve = tokio::task::spawn_blocking(move || gate(&owned, reach));
            match tokio::time::timeout(budget, resolve).await {
                Ok(Ok(gated)) => gated,
                Ok(Err(_)) | Err(_) => gate_of(Err(GateStop::TimedOut)),
            }
        }
    }
}

fn gate_of(answer: Result<(), GateStop>) -> Gate {
    match answer {
        Ok(()) => Gate(None),
        Err(GateStop::Protected(protected)) => Gate(Some(reason(protected))),
        Err(GateStop::TimedOut) => Gate(Some(ProtectedReason::Unresolved)),
    }
}

/// What may leave the machine about the item at `path`, as `gated` allows.
pub(crate) fn for_model(card: &AboutCard, path: &str, gated: Gate) -> ModelCard {
    cmdr_about::model::for_model(card, path, gated.protected(), &AppWords)
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
    use crate::test_support::TestDir;

    fn card() -> AboutCard {
        AboutCard {
            target: AboutTarget::File,
            identity: Some(Fact::Known(Identity {
                name: ItemText::new("id_ed25519"),
                name_hid_reordering: false,
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

    fn leaving(path: &str, reach: GateReach) -> serde_json::Value {
        serde_json::to_value(for_model(&card(), path, gate(path, reach))).expect("serializes")
    }

    /// The host answers the gate itself, so a sender can't hand a credentials
    /// folder's item to a model with its recorded facts.
    #[test]
    fn a_path_the_gate_protects_leaves_name_level_facts_only() {
        for reach in [GateReach::FollowLocalLinks, GateReach::NamesOnly] {
            let leaving = leaving("/Users/jane/.ssh/id_ed25519", reach);
            assert_eq!(leaving["contentsStayHome"], "credentials");
            assert_eq!(
                leaving["realType"],
                serde_json::json!({ "state": "unknown", "value": "protected" })
            );
            assert_eq!(leaving["identity"]["value"]["name"], "id_ed25519");

            let open = self::leaving("/Users/jane/Documents/id_ed25519", reach);
            assert!(open.get("contentsStayHome").is_none());
            assert_eq!(open["realType"], serde_json::json!({ "state": "pending" }));
        }
    }

    /// `inspect_file`'s gate follows a link; so does this one, so a symlink into
    /// `.ssh` reads as the folder it leads to.
    #[cfg(unix)]
    #[test]
    fn a_link_into_a_protected_folder_is_protected_once_followed() {
        let dir = TestDir::new("about-model-link");
        let keys = dir.as_ref().join(".ssh");
        std::fs::create_dir(&keys).expect("make the folder");
        std::fs::write(keys.join("id_ed25519"), "x").expect("write the key");
        let link = dir.as_ref().join("keys");
        std::os::unix::fs::symlink(&keys, &link).expect("link to it");
        let through_link = link.join("id_ed25519").to_string_lossy().into_owned();

        assert_eq!(
            gate(&through_link, GateReach::FollowLocalLinks).protected(),
            Some(ProtectedReason::Credentials)
        );
        // Reading the names alone can't see a link, by design: that reach is only
        // for rows the drive index answers.
        assert_eq!(gate(&through_link, GateReach::NamesOnly).protected(), None);
    }

    /// A gate that can't finish fails closed.
    #[tokio::test]
    async fn a_gate_that_cant_finish_treats_the_item_as_protected() {
        assert_eq!(
            gate_of(Err(GateStop::TimedOut)).protected(),
            Some(ProtectedReason::Unresolved)
        );
        assert_eq!(gate_of(Ok(())).protected(), None);
        let gated = gate_within(
            "/Users/jane/.ssh/id_ed25519",
            GateReach::FollowLocalLinks,
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(gated.protected(), Some(ProtectedReason::Credentials));
    }

    /// Sizes and dates read as other tool results word them.
    #[test]
    fn numbers_arrive_in_the_apps_own_words() {
        let leaving = leaving("/Users/jane/Documents/id_ed25519", GateReach::NamesOnly);
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
