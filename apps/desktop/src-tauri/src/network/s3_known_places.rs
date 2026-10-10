//! The S3 places this user has connected to, so the next time is one click.
//!
//! ❗ **One entry per PLACE, ❌ never per account.** An S3 account (an endpoint
//! plus an access key id) has many places: each bucket, plus the account root
//! that lists them. Each place is its own volume with its own id
//! (`cmdr_fs::volume::s3_volume_id`), pin, and switch, so each is its own entry;
//! the servers listing groups them back under their account
//! (`commands/servers.rs`).
//!
//! ❗ **The NAME is the account's, ❌ never a place's.** A person names the
//! server (the account), and a bucket reads as its own name, the way an SMB
//! host carries the name and its shares read as themselves. So the store keeps
//! one [`KnownS3Account`] per NAMED account beside the places; an unnamed
//! account has no record and reads as `key id@host`.
//!
//! ❌ **No secret lives here.** The secret access key goes to the
//! `CredentialStore` seam, keyed `service = "s3+{scheme}://{host}:{port}"`
//! (`S3ConnectionParams::credential_service`) and `scope = Some(access_key_id)`:
//! the ACCOUNT, so every bucket under one key finds the one secret.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use cmdr_s3::{InvalidProvider, S3ConnectionParams, S3Provider};
use serde::{Deserialize, Serialize};

use super::saved_server_fields::{self, Relocation};
use super::server_list_file;

use crate::ignore_poison::IgnorePoison;

/// Which provider a saved S3 place is on, as the connect form's presets name
/// it. The wire and store twin of `cmdr_s3::S3Provider`.
///
/// ❗ The preset decides the endpoint, the signing region, and the addressing,
/// so only "Other" carries an endpoint at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum S3ProviderChoice {
    /// Amazon S3, in one region (`eu-west-1`).
    Aws {
        /// The endpoint's region.
        region: String,
    },
    /// Cloudflare R2, by account ID.
    R2 {
        /// The 32-hex account ID from the R2 dashboard.
        account_id: String,
    },
    /// Backblaze B2, in one region (`us-west-004`).
    B2 {
        /// The region from the bucket's S3 endpoint.
        region: String,
    },
    /// Wasabi, in one region (`eu-central-1`).
    Wasabi {
        /// The endpoint's region.
        region: String,
    },
    /// Hetzner Object Storage, in one location (`fsn1`, `nbg1`, `hel1`).
    Hetzner {
        /// The endpoint's location.
        location: String,
    },
    /// Google Cloud Storage, through its S3-compatible XML API with HMAC keys.
    Gcs,
    /// DigitalOcean Spaces, in one region (`fra1`).
    #[serde(rename = "digitalocean")]
    DigitalOcean {
        /// The endpoint's region.
        region: String,
    },
    /// Any other S3-compatible server.
    Other {
        /// `http(s)://host[:port]`, nothing after it.
        endpoint: String,
        /// The signing region; `us-east-1` when empty.
        region: Option<String>,
        /// Whether buckets go in the path rather than the host name.
        path_style: bool,
    },
}

impl S3ProviderChoice {
    /// The crate's provider, or `InvalidProvider` for an endpoint that isn't a
    /// URL. The crate checks the rest (host-safe regions, a bare endpoint).
    pub fn to_provider(&self) -> Result<S3Provider, InvalidProvider> {
        let trimmed = |text: &str| text.trim().to_string();
        Ok(match self {
            Self::Aws { region } => S3Provider::Aws {
                region: trimmed(region),
            },
            Self::R2 { account_id } => S3Provider::R2 {
                account_id: trimmed(account_id),
            },
            Self::B2 { region } => S3Provider::B2 {
                region: trimmed(region),
            },
            Self::Wasabi { region } => S3Provider::Wasabi {
                region: trimmed(region),
            },
            Self::Hetzner { location } => S3Provider::Hetzner {
                location: trimmed(location),
            },
            Self::Gcs => S3Provider::Gcs,
            Self::DigitalOcean { region } => S3Provider::DigitalOcean {
                region: trimmed(region),
            },
            Self::Other {
                endpoint,
                region,
                path_style,
            } => S3Provider::Other {
                endpoint: url::Url::parse(endpoint.trim()).map_err(|_| InvalidProvider)?,
                region: region
                    .as_deref()
                    .map(str::trim)
                    .filter(|r| !r.is_empty())
                    .map(str::to_string),
                path_style: *path_style,
            },
        })
    }
}

/// One S3 place the user has connected to, and how to reach it again.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownS3Place {
    /// The provider, and with it the endpoint.
    pub provider: S3ProviderChoice,
    /// The account's key. ❗ Part of the identity.
    pub access_key_id: String,
    /// The bucket this place is, or `None` for the account root that lists the
    /// buckets. ❗ Part of the identity.
    pub bucket: Option<String>,
    /// Whether Cmdr may redial unattended when the session drops. ❗ Defaults to
    /// on, the same as SFTP's and WebDAV's.
    #[serde(default = "reconnects_automatically")]
    pub auto_reconnect: bool,
    /// Whether this place shows in the volume switcher. ❗ A `remember` never
    /// changes it on a replace (the WebDAV store's rule and reason).
    #[serde(default)]
    pub pinned: bool,
    /// When this place was last connected to, ISO 8601.
    pub last_connected_at: String,
}

impl KnownS3Place {
    /// The connection params this entry describes, "reconnect automatically"
    /// included.
    pub fn params(&self) -> Result<S3ConnectionParams, InvalidProvider> {
        let mut params = S3ConnectionParams::new(
            self.provider.to_provider()?,
            &self.access_key_id,
            self.bucket.as_deref(),
        )?;
        params.auto_reconnect = self.auto_reconnect;
        Ok(params)
    }

    /// The volume id this place is registered under, or `None` for a provider
    /// no dial could reach.
    pub fn volume_id(&self) -> Option<String> {
        let params = self.params().ok()?;
        Some(place_id(&params))
    }

    /// The id of the account this place belongs to, or `None` for a provider no
    /// dial could reach.
    fn account_id(&self) -> Option<String> {
        let params = self.params().ok()?;
        Some(account_id(&params))
    }
}

/// A NAMED S3 account: the endpoint plus key the places under it share, and
/// the name a person gave it. ❗ An unnamed account has no record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownS3Account {
    /// The provider, and with it the endpoint.
    pub provider: S3ProviderChoice,
    /// The account's key.
    pub access_key_id: String,
    /// The name a person gave the account, never blank.
    pub display_name: String,
}

impl KnownS3Account {
    /// The id the account is listed under ([`account_id`]), or `None` for a
    /// provider no dial could reach.
    fn id(&self) -> Option<String> {
        let params = S3ConnectionParams::new(self.provider.to_provider().ok()?, &self.access_key_id, None).ok()?;
        Some(account_id(&params))
    }
}

/// The id a place's params register under: `cmdr_fs::volume::s3_volume_id`.
pub fn place_id(params: &S3ConnectionParams) -> String {
    cmdr_fs::volume::s3_volume_id(params.host(), params.port(), params.access_key_id(), params.bucket())
}

/// The id of the ACCOUNT a place belongs to: the account root's place id,
/// which names the account whether or not that place is saved.
pub fn account_id(params: &S3ConnectionParams) -> String {
    cmdr_fs::volume::s3_volume_id(params.host(), params.port(), params.access_key_id(), None)
}

fn reconnects_automatically() -> bool {
    true
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownS3PlacesStore {
    /// Every place, in the order they were first added.
    #[serde(default)]
    pub known_s3_places: Vec<KnownS3Place>,
    /// Every named account, one record each.
    #[serde(default)]
    pub known_s3_accounts: Vec<KnownS3Account>,
}

/// The store as a file may hold it: the current shape, or one whose places
/// still carry a `displayName` of their own, from before the name moved to the
/// account. [`migrate`] turns it into a [`KnownS3PlacesStore`].
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredS3PlacesFile {
    #[serde(default)]
    known_s3_places: Vec<StoredS3Place>,
    #[serde(default)]
    known_s3_accounts: Vec<KnownS3Account>,
}

/// A place as a file may hold it, with the per-place name an older store wrote.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredS3Place {
    #[serde(flatten)]
    place: KnownS3Place,
    #[serde(default)]
    display_name: String,
}

/// The current store, plus whether the file needs writing back because some
/// place still carried its own name.
///
/// ❗ Each named account takes the name of its most recently connected named
/// place, unless it already has a record: two places of one account could each
/// carry a name, and the newest is what the person last typed.
pub fn migrate(file: StoredS3PlacesFile) -> (KnownS3PlacesStore, bool) {
    let mut accounts = file.known_s3_accounts;
    let mut legacy: Vec<&StoredS3Place> = file
        .known_s3_places
        .iter()
        .filter(|stored| saved_server_fields::is_named(&stored.display_name))
        .collect();
    let migrated = !legacy.is_empty();
    legacy.sort_by(|a, b| b.place.last_connected_at.cmp(&a.place.last_connected_at));
    for stored in legacy {
        let Some(id) = stored.place.account_id() else { continue };
        if accounts
            .iter()
            .any(|account| account.id().as_deref() == Some(id.as_str()))
        {
            continue;
        }
        accounts.push(KnownS3Account {
            provider: stored.place.provider.clone(),
            access_key_id: stored.place.access_key_id.clone(),
            display_name: stored.display_name.trim().to_string(),
        });
    }
    let store = KnownS3PlacesStore {
        known_s3_places: file.known_s3_places.into_iter().map(|stored| stored.place).collect(),
        known_s3_accounts: accounts,
    };
    (store, migrated)
}

static KNOWN: OnceLock<Mutex<KnownS3PlacesStore>> = OnceLock::new();
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn known() -> &'static Mutex<KnownS3PlacesStore> {
    KNOWN.get_or_init(|| Mutex::new(KnownS3PlacesStore::default()))
}

/// Loads the store from the app's data dir. Call once at startup; a missing or
/// unreadable file is an empty store.
pub fn load_known_s3_places<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some((file, path)) = server_list_file::load::<_, StoredS3PlacesFile>(app, "s3_known_places.json") else {
        return;
    };
    let (store, migrated) = migrate(file);
    *known().lock_ignore_poison() = store;
    let _ = STORE_PATH.set(path);
    if migrated {
        save();
    }
}

fn save() {
    let Some(path) = STORE_PATH.get() else {
        // Before the load there's nowhere to write; a test binary lands here.
        return;
    };
    let store = known().lock_ignore_poison().clone();
    server_list_file::save(path, "the known S3 places", &store);
}

/// Every place the user has connected to.
pub fn all() -> Vec<KnownS3Place> {
    known().lock_ignore_poison().known_s3_places.clone()
}

/// The entry whose place id is `volume_id`.
pub fn find(volume_id: &str) -> Option<KnownS3Place> {
    all()
        .into_iter()
        .find(|entry| entry.volume_id().as_deref() == Some(volume_id))
}

/// The name a person gave the account `place` belongs to, or empty when
/// nobody did.
pub fn account_name(place: &KnownS3Place) -> String {
    let Some(id) = place.account_id() else {
        return String::new();
    };
    known()
        .lock_ignore_poison()
        .known_s3_accounts
        .iter()
        .find(|account| account.id().as_deref() == Some(id.as_str()))
        .map(|account| account.display_name.clone())
        .unwrap_or_default()
}

/// What the UI calls the account `place` belongs to: its name, else
/// `key id@host` (`saved_server_fields::server_label`).
pub fn account_label(place: &KnownS3Place) -> String {
    let host = place
        .params()
        .map_or_else(|_| String::new(), |params| params.host().to_string());
    saved_server_fields::server_label(&account_name(place), &place.access_key_id, &host)
}

/// What the UI calls a place: a bucket by its own name, exactly as the
/// provider spells it, and the account root by its account's label, since
/// opening the root IS opening the account.
pub fn place_label(place: &KnownS3Place) -> String {
    match place.bucket.as_deref() {
        Some(bucket) => bucket.to_string(),
        None => account_label(place),
    }
}

/// Names `place`'s account with what an add or a connect TYPED, when it typed
/// anything. ❗ A blank Name leaves the account's name alone: adding a second
/// bucket under a named key without retyping its name must not unname it. The
/// newest name typed wins.
pub fn adopt_typed_name(place: &KnownS3Place, name: &str) {
    if !saved_server_fields::is_named(name) {
        return;
    }
    let Some(id) = place.account_id() else {
        return;
    };
    set_account_name(&id, &place.provider, &place.access_key_id, name);
}

/// Renames the account listed as `account_id`, answering whether any saved
/// place belongs to it. ❗ A blank name UNNAMES it (an edit's empty Name field),
/// so it reads as `key id@host` again.
pub fn rename_account(account_id: &str, name: &str) -> bool {
    let Some(place) = all()
        .into_iter()
        .find(|entry| entry.account_id().as_deref() == Some(account_id))
    else {
        return false;
    };
    set_account_name(account_id, &place.provider, &place.access_key_id, name);
    true
}

fn set_account_name(account_id: &str, provider: &S3ProviderChoice, access_key_id: &str, name: &str) {
    let name = name.trim();
    {
        let mut store = known().lock_ignore_poison();
        store
            .known_s3_accounts
            .retain(|account| account.id().as_deref() != Some(account_id));
        if !name.is_empty() {
            store.known_s3_accounts.push(KnownS3Account {
                provider: provider.clone(),
                access_key_id: access_key_id.to_string(),
                display_name: name.to_string(),
            });
        }
    }
    save();
}

/// Whether `entry` is the place `volume_id` names. ❗ By the derived id, the
/// one identity the registry and the listing both use.
fn is_place(entry: &KnownS3Place, volume_id: &str) -> bool {
    entry.volume_id().as_deref() == Some(volume_id)
}

/// Adds `place`, or replaces the entry with the same place id. A place no
/// dial could reach isn't stored.
///
/// ❗ **`place.pinned` is honored only when the entry is NEW**, so a reconnect
/// can't re-pin a place the user unpinned (the WebDAV store's rule).
pub fn remember(mut place: KnownS3Place) {
    let Some(volume_id) = place.volume_id() else {
        log::warn!(target: "volume", "an S3 place with an unusable provider wasn't saved");
        return;
    };
    {
        let mut store = known().lock_ignore_poison();
        match store
            .known_s3_places
            .iter_mut()
            .find(|entry| is_place(entry, &volume_id))
        {
            Some(existing) => {
                place.pinned = existing.pinned;
                *existing = place;
            }
            None => store.known_s3_places.push(place),
        }
    }
    save();
}

/// Applies `change` to the place `volume_id` names, in place under the lock,
/// answering whether there was one. ❗ Never a read-modify-`remember`: that
/// would write back every other field as it stood at the read.
fn update(volume_id: &str, change: impl FnOnce(&mut KnownS3Place)) -> bool {
    let moved = {
        let mut store = known().lock_ignore_poison();
        match store
            .known_s3_places
            .iter_mut()
            .find(|entry| is_place(entry, volume_id))
        {
            Some(entry) => {
                change(entry);
                true
            }
            None => false,
        }
    };
    if moved {
        save();
    }
    moved
}

/// Moves the pin on one place.
pub fn set_pinned(volume_id: &str, pinned: bool) -> bool {
    update(volume_id, |entry| entry.pinned = pinned)
}

/// Moves "reconnect automatically" on one place. The live volume's copy is the
/// wiring's job (`s3_volume_wiring::apply_auto_reconnect`).
pub fn set_auto_reconnect(volume_id: &str, on: bool) -> bool {
    update(volume_id, |entry| entry.auto_reconnect = on)
}

/// Moves the account listed as `account_id` to `provider`'s endpoint: every place
/// under its key, and its name. `previous` holds the places as they stood.
///
/// ❗ The ACCOUNT moves, ❌ never one place: the endpoint and the secret are the
/// account's, so a bucket moved alone would split one account into two. Each place
/// keeps its bucket, pin, switch, and `last_connected_at`. Refused, writing nothing,
/// when another saved account already holds the new endpoint under this key
/// (`Taken`, one of its places), and `NotFound` when nothing is saved under the id
/// or `provider` makes no endpoint. `../server_move.rs` owns what else a move
/// takes along.
pub fn relocate_account(account_id: &str, provider: S3ProviderChoice) -> Relocation<Vec<KnownS3Place>> {
    let relocation = {
        let mut store = known().lock_ignore_poison();
        let moving: Vec<usize> = store
            .known_s3_places
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.account_id().as_deref() == Some(account_id))
            .map(|(i, _)| i)
            .collect();
        let Some(&first) = moving.first() else {
            return Relocation::NotFound;
        };
        let moved_first = KnownS3Place {
            provider: provider.clone(),
            ..store.known_s3_places[first].clone()
        };
        let Some(new_account_id) = moved_first.account_id() else {
            return Relocation::NotFound;
        };
        let holder =
            store.known_s3_places.iter().enumerate().find(|(i, entry)| {
                !moving.contains(i) && entry.account_id().as_deref() == Some(new_account_id.as_str())
            });
        if let Some((_, holder)) = holder {
            return Relocation::Taken(vec![holder.clone()]);
        }
        let previous = moving.iter().map(|&i| store.known_s3_places[i].clone()).collect();
        for &i in &moving {
            store.known_s3_places[i].provider = provider.clone();
        }
        for account in &mut store.known_s3_accounts {
            if account.id().as_deref() == Some(account_id) {
                account.provider = provider.clone();
            }
        }
        Relocation::Moved { previous }
    };
    save();
    relocation
}

/// Drops one place, answering whether it was there. ❌ Leaves the secret alone:
/// other places of the account still sign in with it. The account's name goes
/// with its LAST place, so adding the key again starts unnamed.
pub fn forget(volume_id: &str) -> bool {
    let removed = {
        let mut store = known().lock_ignore_poison();
        let before = store.known_s3_places.len();
        store.known_s3_places.retain(|entry| !is_place(entry, volume_id));
        let still_saved: std::collections::HashSet<String> = store
            .known_s3_places
            .iter()
            .filter_map(KnownS3Place::account_id)
            .collect();
        store
            .known_s3_accounts
            .retain(|account| account.id().is_some_and(|id| still_saved.contains(&id)));
        store.known_s3_places.len() != before
    };
    if removed {
        save();
    }
    removed
}

#[cfg(test)]
#[path = "s3_known_places_test.rs"]
mod s3_known_places_test;
