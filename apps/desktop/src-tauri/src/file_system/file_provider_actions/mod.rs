//! Every custom action a File Provider extension declares (Dropbox, Google Drive, MacDroid,
//! any provider), offered in the file context menu the way Finder offers them.
//!
//! [`offer_for`] answers what the menu shows for the right-clicked rows; [`perform`] runs
//! the one the user picked. `objc.rs` holds the private mechanism and its evidence;
//! `declarations.rs` holds the pure rules.
//!
//! **The gate, cheapest first:**
//!
//! 1. Every row sits under ONE domain's storage root, read from a cached registry of
//!    domains. An ordinary folder stops here without a single XPC round trip, and a hung
//!    `fileproviderd` can't slow it down: the registry refreshes on a thread of its own and
//!    the menu reads whatever that last produced.
//! 2. File Provider resolves every row to an item, within the menu's budget.
//! 3. Each declared rule is evaluated against those items and the domain's live
//!    `domainUserInfo`. The matches, minus the ones Cmdr has its own items for, are the offer.
//!
//! **Why storage roots are a sound pre-gate:** a replicated domain's items live under the
//! `storageURLs` File Provider reports for it (`~/Library/CloudStorage/<name>` for Dropbox,
//! Google Drive, and MacDroid; `~/Library/Mobile Documents` for iCloud Drive), and an item
//! can't exist outside its domain. The roots come from `fileproviderd` itself, so a domain on
//! an external volume, or a provider installed later, needs no path table. A row reached
//! through a symlink near the top of home (`~/Dropbox`) is resolved first. Mirror-mode Google
//! Drive keeps plain local files under no root, so it never reaches File Provider.

mod declarations;
mod objc;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::ignore_poison::RwLockIgnorePoison as _;
use declarations::{ActionDeclaration, Declarations};

const LOG_TARGET: &str = "file_provider_actions";

/// How old the domain registry may get before a menu asks for a fresh one. Domains change
/// when a provider signs in, signs out, or gets installed, so this only bounds how long a new
/// one takes to show up.
const REGISTRY_TTL: Duration = Duration::from_secs(30);

/// The largest selection the group is offered for. Each row costs an XPC round trip, so a
/// right-click on 10,000 selected files mustn't send 10,000 of them.
const MAX_SELECTION: usize = 100;

/// File Provider calls stay off constrained-stack pools (`file_system/CLAUDE.md`).
const THREAD_STACK_BYTES: usize = 8 * 1024 * 1024;

/// What the file context menu offers for the right-clicked rows: one provider's actions
/// that apply to all of them, already labeled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOffer {
    /// The domain the rows belong to, which every action is addressed to.
    pub provider_domain_id: String,
    /// File Provider's identifiers for the rows, in selection order.
    pub item_identifiers: Vec<String>,
    /// The matching actions, in the provider's declared order, minus Cmdr's hidden ones.
    pub actions: Vec<OfferedAction>,
}

/// One action as a menu line needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedAction {
    pub identifier: String,
    /// The provider's own label, in Cmdr's UI language when the provider has it.
    pub label: String,
}

/// Why a click didn't get as far as File Provider.
#[derive(Debug, PartialEq, Eq)]
pub enum PerformError {
    /// The click named an index the offer doesn't have.
    NoSuchAction,
    /// The thread that talks to File Provider couldn't start.
    NoThread,
}

/// Starts reading the domain registry, so the first right-click in a provider's folder
/// already finds it. Returns at once.
pub fn warm() {
    refresh_in_background();
}

/// The provider actions the menu offers for `paths`, waiting at most `budget` on File
/// Provider. `None` means no group: a row outside every domain, rows in two domains, a
/// provider that answered too late, or nothing that applies.
pub fn offer_for(paths: &[PathBuf], budget: Duration) -> Option<ProviderOffer> {
    if paths.is_empty() || paths.len() > MAX_SELECTION {
        return None;
    }
    let registry = current_registry()?;
    let home = dirs::home_dir();
    let (index, visible) = declarations::domain_for_paths(&registry.roots, paths, home.as_deref(), |path| {
        std::fs::read_link(path).ok()
    })?;
    let domain = registry.domains.get(index)?;

    let deadline = Instant::now() + budget;
    let items = objc::fetch_items(&visible, deadline)?;
    if items.iter().any(|item| item.domain_id != domain.id) {
        return None;
    }
    let user_info = objc::fetch_domain_user_info(&domain.id, deadline)?;

    let matched: Vec<&ActionDeclaration> =
        objc::matching_actions(&domain.extension, &domain.declarations, &items, &user_info.0)
            .into_iter()
            .filter_map(|position| domain.declarations.actions.get(position))
            .filter(|action| !declarations::is_hidden_by_cmdr(&domain.provider_id, &action.identifier))
            .collect();
    if matched.is_empty() {
        return None;
    }

    let tables = objc::label_tables(
        &domain.extension,
        &domain.declarations.version,
        &crate::intl::active_locale(),
    );
    let tables: Vec<_> = tables.iter().collect();
    Some(ProviderOffer {
        provider_domain_id: domain.id.clone(),
        item_identifiers: items.iter().map(|item| item.identifier.clone()).collect(),
        actions: matched
            .iter()
            .map(|action| OfferedAction {
                identifier: action.identifier.clone(),
                label: declarations::resolve_label(&action.name_key, &tables),
            })
            .collect(),
    })
}

/// Runs the offer's action at `index` on the rows the offer was evaluated for.
///
/// On a thread of its own: building and scheduling the operation talks to `fileproviderd`,
/// and a menu click must never wait on it. The provider shows its own windows, so nothing
/// here needs the main thread.
pub fn perform(offer: &ProviderOffer, index: usize) -> Result<(), PerformError> {
    let action = offer.actions.get(index).ok_or(PerformError::NoSuchAction)?;
    let identifier = action.identifier.clone();
    let domain_id = offer.provider_domain_id.clone();
    let items = offer.item_identifiers.clone();
    std::thread::Builder::new()
        .name("file-provider-action".to_string())
        .stack_size(THREAD_STACK_BYTES)
        .spawn(move || match objc::schedule(&identifier, &domain_id, &items) {
            Ok(()) => {
                log::debug!(target: LOG_TARGET, "Scheduled {identifier} for {} item(s) in {domain_id}", items.len())
            }
            Err(reason) => log::warn!(target: LOG_TARGET, "File Provider action {identifier} didn't start: {reason:?}"),
        })
        .map(drop)
        .map_err(|_| PerformError::NoThread)
}

/// One domain that declares at least one action.
#[derive(Clone)]
struct Domain {
    id: String,
    provider_id: String,
    extension: PathBuf,
    declarations: Declarations,
}

/// What the menu path reads: every acting domain, with its roots split out for the pre-gate.
struct Registry {
    /// `roots[i]` belongs to `domains[i]`.
    roots: Vec<Vec<PathBuf>>,
    domains: Vec<Domain>,
    refreshed_at: Instant,
}

static REGISTRY: RwLock<Option<Arc<Registry>>> = RwLock::new(None);

/// Single flight: a `fileproviderd` that never answers can hold one refresh thread, never more.
static REFRESHING: AtomicBool = AtomicBool::new(false);

/// The registry as last read, kicking a refresh when it's missing or stale. Never waits:
/// the very first right-click after launch, before the warm-up lands, simply gets no group.
fn current_registry() -> Option<Arc<Registry>> {
    let current = REGISTRY.read_ignore_poison().clone();
    if current
        .as_ref()
        .is_none_or(|registry| registry.refreshed_at.elapsed() > REGISTRY_TTL)
    {
        refresh_in_background();
    }
    current
}

fn refresh_in_background() {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("file-provider-domains".to_string())
        .stack_size(THREAD_STACK_BYTES)
        .spawn(|| {
            refresh_registry();
            REFRESHING.store(false, Ordering::SeqCst);
        });
    if let Err(error) = spawned {
        REFRESHING.store(false, Ordering::SeqCst);
        log::warn!(target: LOG_TARGET, "Couldn't start the File Provider domain refresh: {error}");
    }
}

/// Lists the domains and reads what each extension declares. Synchronous XPC plus a small
/// file read per extension, so ❌ never on the menu path.
fn refresh_registry() {
    let Some(listed) = objc::list_domains() else {
        // Keep what we had, and don't ask again for a TTL: a framework that didn't answer
        // won't start answering by the next right-click.
        log::debug!(target: LOG_TARGET, "File Provider didn't list its domains; keeping the last registry");
        let mut slot = REGISTRY.write_ignore_poison();
        let (roots, domains) = slot
            .as_deref()
            .map(|registry| (registry.roots.clone(), registry.domains.clone()))
            .unwrap_or_default();
        *slot = Some(Arc::new(Registry {
            roots,
            domains,
            refreshed_at: Instant::now(),
        }));
        return;
    };
    let (roots, domains): (Vec<_>, Vec<_>) = listed
        .into_iter()
        .filter_map(|listed| {
            let bytes = std::fs::read(listed.extension.join("Contents/Info.plist")).ok()?;
            let declarations = declarations::parse_declarations(&bytes)?;
            // iCloud Drive declares no actions, so its domain never costs a right-click a fetch.
            (!declarations.actions.is_empty()).then_some({
                (
                    listed.roots,
                    Domain {
                        id: listed.id,
                        provider_id: listed.provider_id,
                        extension: listed.extension,
                        declarations,
                    },
                )
            })
        })
        .unzip();
    *REGISTRY.write_ignore_poison() = Some(Arc::new(Registry {
        roots,
        domains,
        refreshed_at: Instant::now(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_path(variable: &str) -> PathBuf {
        PathBuf::from(std::env::var(variable).unwrap_or_else(|_| panic!("set {variable} to a real item")))
    }

    fn identifiers(offer: &ProviderOffer) -> Vec<&str> {
        offer.actions.iter().map(|action| action.identifier.as_str()).collect()
    }

    #[test]
    fn performing_an_index_past_the_offer_refuses() {
        let offer = ProviderOffer {
            provider_domain_id: "domain".to_string(),
            item_identifiers: vec!["item".to_string()],
            actions: Vec::new(),
        };
        assert_eq!(perform(&offer, 0), Err(PerformError::NoSuchAction));
    }

    /// Manual test against a REAL streamed Google Drive item Drive lets you share, so it's
    /// ignored by default. Run it with:
    ///
    /// `CMDR_DRIVE_STREAM_ITEM=<path> cargo nextest run --workspace --features cmdr/virtual-mtp --run-ignored only -E 'test(a_real_drive_item_offers_share_without_cmdrs_duplicates)'`
    ///
    /// Also the crash regression: a completion handler without a type signature became nil
    /// inside File Provider, which crashed the process when the reply arrived (`EXC_BAD_ACCESS`
    /// at `0x10`); pre-fix, the first fetch died with SIGSEGV. The 1 ms waits let replies land
    /// after the caller gave up, the way the context menu's bounded wait does.
    #[test]
    #[ignore = "needs a streamed Google Drive item in CMDR_DRIVE_STREAM_ITEM"]
    fn a_real_drive_item_offers_share_without_cmdrs_duplicates() {
        refresh_registry();
        let item = real_path("CMDR_DRIVE_STREAM_ITEM");
        for _ in 0..20 {
            let _ = offer_for(std::slice::from_ref(&item), Duration::from_millis(1));
        }
        // allowed-test-sleep: the replies landing after their callers stopped waiting are the subject
        std::thread::sleep(Duration::from_secs(5));

        let offer = offer_for(&[item], Duration::from_secs(5)).expect("a shareable streamed Drive item gets an offer");
        let ids = identifiers(&offer);
        assert!(ids.contains(&"ACTION_SHARE"), "Drive's Share is offered: {ids:?}");
        for duplicate in ["ACTION_OPEN", "ACTION_COPY_LINK", "ACTION_OPEN_GEMINI_WEB"] {
            assert!(
                !ids.contains(&duplicate),
                "Cmdr has its own item for {duplicate}: {ids:?}"
            );
        }
    }

    /// Manual test against REAL Dropbox items, so it's ignored by default. Run it with:
    ///
    /// `CMDR_DROPBOX_FILE=<file> CMDR_DROPBOX_FOLDER=<folder> cargo nextest run --workspace --features cmdr/virtual-mtp --run-ignored only -E 'test(real_dropbox_items_offer_dropboxs_own_actions)'`
    #[test]
    #[ignore = "needs Dropbox items in CMDR_DROPBOX_FILE and CMDR_DROPBOX_FOLDER"]
    fn real_dropbox_items_offer_dropboxs_own_actions() {
        refresh_registry();
        let file = real_path("CMDR_DROPBOX_FILE");
        let folder = real_path("CMDR_DROPBOX_FOLDER");
        let copy_link = "com.getdropbox.dropbox.fileprovider.action.copy_link";
        let budget = Duration::from_secs(5);

        let file_offer = offer_for(std::slice::from_ref(&file), budget).expect("a Dropbox file gets an offer");
        assert!(
            identifiers(&file_offer).contains(&copy_link),
            "{:?}",
            file_offer.actions
        );
        assert!(
            file_offer
                .actions
                .iter()
                .all(|action| !action.label.is_empty() && !action.label.ends_with("_NAME")),
            "every label comes from Dropbox's strings: {:?}",
            file_offer.actions
        );

        assert!(
            offer_for(std::slice::from_ref(&folder), budget).is_some(),
            "a Dropbox folder gets an offer"
        );

        // One file and one folder: single-item rules drop out, whole-selection ones stay.
        let mixed = offer_for(&[file, folder], budget).expect("a mixed Dropbox selection gets an offer");
        assert!(!identifiers(&mixed).contains(&copy_link), "{:?}", mixed.actions);
        assert_eq!(mixed.item_identifiers.len(), 2);
    }
}
