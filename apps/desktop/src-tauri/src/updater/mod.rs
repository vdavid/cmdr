//! Custom macOS updater that preserves TCC/Full Disk Access permissions across updates.
//!
//! Instead of replacing the entire `.app` bundle (which changes its inode and causes macOS
//! to lose track of FDA grants), this updater syncs files *into* the existing bundle,
//! preserving the directory inode and `com.apple.macl` xattr.
//!
//! Three Tauri commands:
//! - `check_for_update`: fetches `latest.json`, compares versions
//! - `download_update`: downloads tarball, verifies minisign signature
//! - `install_update`: extracts and syncs into the running `.app` bundle

mod bundle_location;
// Crate-visible for `installer::running_bundle`, which `dock/` needs to decide whether a Dock tile
// could point at this copy. Nothing else outside `updater` reaches in.
pub(crate) mod installer;
mod manifest;
mod signature;

pub use bundle_location::BundleWriteBlocker;
use manifest::UpdateInfo;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::State;

// Per-call timeouts for the manifest fetch. The default `reqwest::get` client has no
// overall timeout. A stuck TCP handshake against the redirect target can hang for
// minutes before the OS gives up. These bounds keep a flaky network from looking like
// a hung app and stop the auto-error-reporter from firing on long hangs.
const MANIFEST_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MANIFEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// Per-call timeouts for the tarball download. No overall `timeout` here: a 60+ MB
// download on a slow connection can legitimately take minutes. `read_timeout` bounds
// "no bytes received in N seconds" instead, which catches mid-download stalls without
// punishing slow-but-working networks.
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(30);

use crate::ignore_poison::IgnorePoison as _;
use crate::managed_policy::{Egress, UpdatePolicy, UpdateRefusal};
use crate::server_request::describe_error_chain;

/// What the latest check offered and what the latest download staged, shared by the three
/// commands. The backend keeps both so a download fetches only what a check offered (never a URL
/// the frontend names) and an install knows which version it's about to write.
pub struct UpdateState {
    slots: Mutex<UpdateSlots>,
}

#[derive(Default)]
struct UpdateSlots {
    offered: Option<UpdateInfo>,
    downloaded: Option<DownloadedUpdate>,
}

/// A verified tarball waiting for `install_update`, and the version the check said it holds.
#[derive(Debug)]
struct DownloadedUpdate {
    version: semver::Version,
    tarball: PathBuf,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(UpdateSlots::default()),
        }
    }

    // A poisoned lock only means an earlier command panicked mid-write; every slot is plain data
    // that the next check or download replaces whole.
    fn slots(&self) -> std::sync::MutexGuard<'_, UpdateSlots> {
        self.slots.lock_ignore_poison()
    }
}

/// What set an update check going. The frontend's `update_check` analytics event carries the same
/// token, and the backend reads it to refuse a background check under
/// `DisableAutomaticUpdateChecks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateCheckTrigger {
    /// The first wake of the poll loop as the app comes up.
    Startup,
    /// A background tick of the poll loop.
    Poll,
    /// `updates.autoCheck` going from off to on.
    AutoCheckOn,
    /// The `app.checkForUpdates` command (menu, command palette, shortcut).
    Command,
    /// The "Check for updates" button on Settings > Updates.
    Settings,
}

impl UpdateCheckTrigger {
    /// Whether the check is the automatic checking `DisableAutomaticUpdateChecks` turns off, as
    /// opposed to a person asking.
    fn is_automatic(self) -> bool {
        match self {
            Self::Startup | Self::Poll | Self::AutoCheckOn => true,
            Self::Command | Self::Settings => false,
        }
    }
}

/// What one update check found. The managed outcomes are answers, not failures: the frontend
/// renders them and ❌ never logs them at warn or error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpdateCheckOutcome {
    /// Nothing newer than what's running (or this isn't a production install, see `skip_reason`).
    UpToDate,
    /// A newer release this Mac may install. `download_update` fetches exactly this one.
    Available { version: String },
    /// A newer release is out, but `MaxUpdateVersion` holds this Mac at `ceiling` or earlier.
    HeldByPolicy { available: String, ceiling: String },
    /// `DisableUpdates`: no request was made.
    UpdatesDisabledByPolicy,
    /// `DisableAutomaticUpdateChecks` refused a background check: no request was made. A check a
    /// person asks for still runs.
    AutomaticChecksDisabledByPolicy,
}

/// Why this process must not run an update check. Carried (rather than collapsed to a bool) so
/// the log names the exact condition that fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipReason {
    /// The executable isn't inside a `.app` bundle, so the install can't possibly succeed.
    NotAnAppBundle,
    /// One of [`crate::prod_instance::NON_PROD_ENV_VARS`] is set in this process's environment.
    NonProdEnv(&'static str),
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnAppBundle => f.write_str("not running from a .app bundle"),
            Self::NonProdEnv(name) => write!(f, "{name} is set"),
        }
    }
}

/// Pure core of the gate, with `in_app_bundle` and `env_is_set` injected so the matrix is
/// unit-testable without mutating the process environment or faking a bundle on disk.
///
/// The bundle condition is checked first: it's the one that makes an update impossible rather
/// than merely unwanted, so it's the more useful thing to see in a log.
fn skip_reason_for(in_app_bundle: bool, env_is_set: &dyn Fn(&str) -> bool) -> Option<SkipReason> {
    if !in_app_bundle {
        return Some(SkipReason::NotAnAppBundle);
    }
    crate::prod_instance::non_prod_env_var_in(env_is_set).map(SkipReason::NonProdEnv)
}

/// The gate against this process's real environment and executable location.
fn skip_reason() -> Option<SkipReason> {
    skip_reason_for(installer::is_running_from_app_bundle(), &|name| {
        std::env::var_os(name).is_some()
    })
}

/// Fetches `latest.json` (via the update check proxy for analytics) and says what it found, with
/// the organization's policy applied. An `Available` release is remembered for `download_update`.
///
/// Answers `UpToDate` when this isn't a real user's production install ([`skip_reason`]): the
/// executable isn't inside a `.app` bundle (dev builds: install can't possibly succeed), or one of
/// [`crate::prod_instance::NON_PROD_ENV_VARS`] is set. Every check reaches
/// `api.getcmdr.com/update-check`, which writes an `update_checks` row that the dashboard counts as
/// an active install, so Cmdr's own runs must never call it. Also `UpToDate` when the remote version
/// isn't newer, or the manifest has no entry for this platform.
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(
    trigger: UpdateCheckTrigger,
    state: State<'_, UpdateState>,
) -> Result<UpdateCheckOutcome, crate::server_request::ServerRequestError> {
    let current_version = env!("CARGO_PKG_VERSION");
    run_check(trigger, &state, skip_reason(), current_version, || async move {
        log::info!("Checking for updates (current version: {current_version})");
        let arch = manifest::platform_key().strip_prefix("darwin-").unwrap_or("unknown");
        let url = format!("https://api.getcmdr.com/update-check/{current_version}?arch={arch}");
        fetch_manifest(&url).await
    })
    .await
}

/// The check with its environment injected, so tests can run every policy without a bundle or a
/// network. `fetch` runs only when the policy and `skip` let the check reach the network.
///
/// The policy goes FIRST, ahead of `skip`: a dev build run with `CMDR_MANAGED_PREFS_FILE` then
/// shows the managed answer, which is what a person testing a profile wants to see.
async fn run_check<F, Fut>(
    trigger: UpdateCheckTrigger,
    state: &UpdateState,
    skip: Option<SkipReason>,
    current_version: &str,
    fetch: F,
) -> Result<UpdateCheckOutcome, crate::server_request::ServerRequestError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<manifest::UpdateManifest, crate::server_request::ServerRequestError>>,
{
    // Every check replaces the last offer, so a download can only fetch what the newest check
    // offered under the newest policy.
    state.slots().offered = None;

    let policy = crate::managed_policy::for_egress().await;
    match policy.updates() {
        UpdatePolicy::Disabled => {
            log::info!(target: "managed_policy", "Not checking for updates: the organization's policy turns updates off");
            return Ok(UpdateCheckOutcome::UpdatesDisabledByPolicy);
        }
        UpdatePolicy::Enabled {
            automatic_checks: false,
            ..
        } if trigger.is_automatic() => {
            log::info!(target: "managed_policy", "Not running the {trigger:?} update check: the organization's policy turns automatic checks off");
            return Ok(UpdateCheckOutcome::AutomaticChecksDisabledByPolicy);
        }
        UpdatePolicy::Enabled { .. } => {}
    }

    if let Some(reason) = skip {
        log::info!("Skipping update check: {reason}");
        return Ok(UpdateCheckOutcome::UpToDate);
    }

    let manifest = fetch().await?;
    let Some(update) = manifest::check_manifest(&manifest, current_version) else {
        return Ok(UpdateCheckOutcome::UpToDate);
    };
    let version = update.version.to_string();
    match policy.update_to(&update.version) {
        Ok(()) => {
            state.slots().offered = Some(update);
            Ok(UpdateCheckOutcome::Available { version })
        }
        Err(UpdateRefusal::AboveCeiling(ceiling)) => {
            log::info!(target: "managed_policy", "Update {version} is out, but the organization's policy holds this Mac at {ceiling} or earlier");
            Ok(UpdateCheckOutcome::HeldByPolicy {
                available: version,
                ceiling: ceiling.to_string(),
            })
        }
        // The policy said "enabled" a moment ago and it's the same read, so this can't happen;
        // answering it truthfully costs nothing.
        Err(UpdateRefusal::Disabled) => Ok(UpdateCheckOutcome::UpdatesDisabledByPolicy),
    }
}

/// Fetches and parses the manifest at `url`. Split from the command so a test can point it at a mock
/// server instead of the update endpoint.
///
/// The status is checked before the body is parsed (`server_request::send`): a 5xx or an HTML
/// maintenance page would otherwise read as "the manifest is malformed" and send whoever reads the log
/// to the wrong layer. A 2xx that doesn't parse is `BadResponse`, which the frontend logs at error:
/// Cmdr's server and this build disagree on the contract. The frontend owns the log line, gated once
/// per condition, so a Rust warn here would only repeat it every poll tick.
async fn fetch_manifest(url: &str) -> Result<manifest::UpdateManifest, crate::server_request::ServerRequestError> {
    let client = cmdr_http::client_builder()
        .connect_timeout(MANIFEST_CONNECT_TIMEOUT)
        .timeout(MANIFEST_REQUEST_TIMEOUT)
        .build()
        .map_err(|e| {
            crate::server_request::ServerRequestError::unexpected(format!(
                "update HTTP client: {}",
                describe_error_chain(&e)
            ))
        })?;
    let response = crate::server_request::send(Egress::UpdateCheck, client.get(url)).await?;
    crate::server_request::read_json(response).await
}

/// Reports whether the running bundle sits somewhere an update can be written into, or `None`
/// when nothing is in the way.
///
/// The frontend asks after a check finds an update and before the download starts. Skipping the
/// download is the point: an install that can't write its own bundle would otherwise pull ~63 MB
/// and rewrite nothing, once per poll interval, for as long as the app runs. It also gives the
/// user a reason for a failure they'd otherwise never see, since neither arrangement can be fixed
/// from inside the app.
///
/// Returns `None` outside a `.app` bundle too: there's no bundle to classify, and the check gate
/// (`skip_reason`) has already stopped that process from getting here.
#[tauri::command]
#[specta::specta]
pub async fn update_write_blocker() -> Result<Option<BundleWriteBlocker>, String> {
    let Ok(bundle) = installer::running_bundle() else {
        return Ok(None);
    };
    let blocker = bundle_location::classify(&bundle);
    if let Some(reason) = blocker {
        log::warn!(
            "Can't install updates into {}: {reason}. The user needs to move Cmdr to Applications.",
            bundle.display()
        );
    }
    Ok(blocker)
}

/// Why a tarball download didn't leave a verified file behind. The frontend picks the log level
/// off the variant: a `Request` failure follows the api-server rule (no network, a timeout, or a
/// 5xx is the person's network or the host's bad moment, so warn), while a signature mismatch or a
/// disk failure means something is wrong with the release or this machine, so error.
///
/// ❌ `detail` is for logs only, never a sentence a person reads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpdateDownloadError {
    /// The tarball request didn't come back with the bytes.
    Request {
        failure: crate::server_request::ServerRequestError,
    },
    /// The bytes arrived but don't verify against the manifest's signature.
    SignatureMismatch { detail: String },
    /// The verified tarball couldn't be written to the temp dir.
    Disk { detail: String },
    /// No check has offered an update since the last one: the frontend asked out of turn.
    NothingOffered,
    /// The organization's policy, read fresh, no longer allows the offered version (it arrived
    /// after the check). Nothing was fetched. Not a failure: ❌ never log it at warn or error.
    BlockedByPolicy,
}

impl From<crate::server_request::ServerRequestError> for UpdateDownloadError {
    fn from(failure: crate::server_request::ServerRequestError) -> Self {
        Self::Request { failure }
    }
}

/// Downloads the tarball at `url` and verifies it against `signature`. Split from the command so a
/// test can point it at a mock server.
///
/// The status is checked before the bytes are trusted (`server_request::send`), so a 5xx
/// maintenance page reads as the host's bad moment rather than as a tarball that fails its
/// signature.
async fn fetch_verified_tarball(url: &str, signature: &str) -> Result<Vec<u8>, UpdateDownloadError> {
    let client = cmdr_http::client_builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_READ_TIMEOUT)
        .build()
        .map_err(|e| {
            crate::server_request::ServerRequestError::unexpected(format!(
                "update HTTP client: {}",
                describe_error_chain(&e)
            ))
        })?;

    let response = crate::server_request::send(Egress::UpdateDownload, client.get(url)).await?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| crate::server_request::ServerRequestError::from_transport(&e))?;

    log::info!("Downloaded {} bytes, verifying signature", bytes.len());
    signature::verify(&bytes, signature).map_err(|detail| UpdateDownloadError::SignatureMismatch { detail })?;
    log::info!("Signature verified");
    // Zero-copy when the buffer is uniquely owned, which a freshly read body is.
    Ok(Vec::from(bytes))
}

/// Downloads the update the last check offered and verifies its minisign signature. Takes no URL:
/// the backend fetches only what it offered, so a bypassed frontend can't stage anything else.
///
/// On success, records the tarball and its version in `UpdateState` for `install_update`.
#[tauri::command]
#[specta::specta]
pub async fn download_update(state: State<'_, UpdateState>) -> Result<(), UpdateDownloadError> {
    let offer = offer_to_download(&state).await?;
    log::info!("Downloading update {} from {}", offer.version, offer.url);

    let bytes = fetch_verified_tarball(&offer.url, &offer.signature).await?;

    let temp_dir = std::env::temp_dir().join("cmdr-update");
    std::fs::create_dir_all(&temp_dir).map_err(|e| UpdateDownloadError::Disk {
        detail: format!("couldn't create temp dir: {e}"),
    })?;

    let tarball_path = temp_dir.join("Cmdr.app.tar.gz");
    std::fs::write(&tarball_path, &bytes).map_err(|e| UpdateDownloadError::Disk {
        detail: format!("couldn't write tarball: {e}"),
    })?;

    state.slots().downloaded = Some(DownloadedUpdate {
        version: offer.version,
        tarball: tarball_path,
    });
    Ok(())
}

/// The offered update, if the policy, read fresh, still allows its version. A profile that arrived
/// between the check and the download stops it here, before a byte is fetched.
async fn offer_to_download(state: &UpdateState) -> Result<UpdateInfo, UpdateDownloadError> {
    let offer = state
        .slots()
        .offered
        .clone()
        .ok_or(UpdateDownloadError::NothingOffered)?;
    if let Err(refusal) = crate::managed_policy::for_egress().await.update_to(&offer.version) {
        log::info!(target: "managed_policy", "Not downloading update {}: the organization's policy refuses it ({refusal:?})", offer.version);
        return Err(UpdateDownloadError::BlockedByPolicy);
    }
    Ok(offer)
}

/// Why `install_update` didn't install. `Failed` covers everything local (extraction, the version
/// check, the sync), which the frontend logs at error.
///
/// ❌ `detail` is for logs only, never a sentence a person reads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpdateInstallError {
    /// The organization's policy, read fresh, doesn't allow the staged version. Nothing was
    /// written. Not a failure: ❌ never log it at warn or error.
    BlockedByPolicy,
    /// No verified download is waiting: the frontend asked out of turn.
    NothingStaged,
    /// The install ran and didn't finish.
    Failed { detail: String },
}

impl From<String> for UpdateInstallError {
    fn from(detail: String) -> Self {
        Self::Failed { detail }
    }
}

/// Installs a previously downloaded update by syncing files into the running `.app` bundle.
///
/// Takes the download `download_update` recorded, and refuses it when the CURRENT policy doesn't
/// allow its version: a download staged before a profile arrived must not install.
#[tauri::command]
#[specta::specta]
pub async fn install_update(state: State<'_, UpdateState>) -> Result<(), UpdateInstallError> {
    let (staged, policy) = staged_to_install(&state).await?;
    log::info!("Installing update {} from {}", staged.version, staged.tarball.display());

    // The archive's own `Info.plist` is what binds its version (the manifest isn't signed), so the
    // installer asks the policy again about THAT version once it has extracted it.
    tokio::task::spawn_blocking(move || {
        installer::install(&staged.tarball, &|version| policy.update_to(version).is_ok())
    })
    .await
    .map_err(|e| UpdateInstallError::Failed {
        detail: format!("Install task panicked: {e}"),
    })?
}

/// The staged download and the fresh policy that allows it, or why not.
async fn staged_to_install(
    state: &UpdateState,
) -> Result<(DownloadedUpdate, std::sync::Arc<crate::managed_policy::ManagedPolicy>), UpdateInstallError> {
    let staged = state
        .slots()
        .downloaded
        .take()
        .ok_or(UpdateInstallError::NothingStaged)?;
    let policy = crate::managed_policy::for_egress().await;
    if let Err(refusal) = policy.update_to(&staged.version) {
        log::info!(target: "managed_policy", "Not installing update {}: the organization's policy refuses it ({refusal:?})", staged.version);
        return Err(UpdateInstallError::BlockedByPolicy);
    }
    Ok((staged, policy))
}

#[cfg(test)]
mod tests;
