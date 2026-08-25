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
mod installer;
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

/// Renders an error and its full `source()` chain. `reqwest::Error`'s `Display` only
/// prints the outermost layer (`error sending request for url …`), which hides the
/// real cause (DNS lookup, TCP connect timeout, TLS handshake, etc.).
fn describe_error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = err.to_string();
    let mut src = err.source();
    while let Some(cause) = src {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        src = cause.source();
    }
    out
}

/// Shared state between `download_update` and `install_update`.
/// Holds the path to the downloaded (and verified) tarball.
pub struct UpdateState {
    downloaded_tarball: Mutex<Option<PathBuf>>,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            downloaded_tarball: Mutex::new(None),
        }
    }
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

/// Fetches `latest.json` (via the update check proxy for analytics) and returns update info
/// if a newer version is available.
///
/// Returns `None` when:
/// - This isn't a real user's production install ([`skip_reason`]): the executable isn't inside a
///   `.app` bundle (dev builds: install can't possibly succeed, so there's no point checking and
///   no point letting the user click "Update"), or one of
///   [`crate::prod_instance::NON_PROD_ENV_VARS`] is set. Every check reaches
///   `api.getcmdr.com/update-check`, which writes an `update_checks` row that the dashboard counts
///   as an active install, so Cmdr's own runs must never call it.
/// - The remote version is not newer than the current version
/// - The manifest doesn't contain an entry for this platform
#[tauri::command]
#[specta::specta]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    if let Some(reason) = skip_reason() {
        log::info!("Skipping update check: {reason}");
        return Ok(None);
    }

    let current_version = env!("CARGO_PKG_VERSION");
    log::info!("Checking for updates (current version: {current_version})");

    let arch = manifest::platform_key().strip_prefix("darwin-").unwrap_or("unknown");
    let url = format!("https://api.getcmdr.com/update-check/{current_version}?arch={arch}");

    let client = reqwest::Client::builder()
        .connect_timeout(MANIFEST_CONNECT_TIMEOUT)
        .timeout(MANIFEST_REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Couldn't build update HTTP client: {}", describe_error_chain(&e)))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Couldn't fetch update manifest: {}", describe_error_chain(&e)))?;

    let manifest: manifest::UpdateManifest = response
        .json()
        .await
        .map_err(|e| format!("Couldn't parse update manifest: {}", describe_error_chain(&e)))?;

    Ok(manifest::check_manifest(&manifest, current_version))
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

/// Downloads the update tarball and verifies its minisign signature.
///
/// On success, stores the tarball path in `UpdateState` for `install_update` to consume.
#[tauri::command]
#[specta::specta]
pub async fn download_update(url: String, signature: String, state: State<'_, UpdateState>) -> Result<(), String> {
    log::info!("Downloading update from {url}");

    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_READ_TIMEOUT)
        .build()
        .map_err(|e| format!("Couldn't build update HTTP client: {}", describe_error_chain(&e)))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Couldn't download update: {}", describe_error_chain(&e)))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Couldn't read update response: {}", describe_error_chain(&e)))?;

    log::info!("Downloaded {} bytes, verifying signature", bytes.len());
    signature::verify(&bytes, &signature)?;
    log::info!("Signature verified");

    let temp_dir = std::env::temp_dir().join("cmdr-update");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("Couldn't create temp dir: {e}"))?;

    let tarball_path = temp_dir.join("Cmdr.app.tar.gz");
    std::fs::write(&tarball_path, &bytes).map_err(|e| format!("Couldn't write tarball: {e}"))?;

    let mut guard = state
        .downloaded_tarball
        .lock()
        .map_err(|e| format!("Couldn't lock update state: {e}"))?;
    *guard = Some(tarball_path);

    Ok(())
}

/// Installs a previously downloaded update by syncing files into the running `.app` bundle.
///
/// Reads (and clears) the tarball path stored by `download_update`.
#[tauri::command]
#[specta::specta]
pub async fn install_update(state: State<'_, UpdateState>) -> Result<(), String> {
    let tarball_path = {
        let mut guard = state
            .downloaded_tarball
            .lock()
            .map_err(|e| format!("Couldn't lock update state: {e}"))?;
        guard.take().ok_or_else(|| "No update downloaded".to_string())?
    };

    log::info!("Installing update from {}", tarball_path.display());

    // Run the install on a blocking thread since it does filesystem I/O

    tokio::task::spawn_blocking(move || installer::install(&tarball_path))
        .await
        .map_err(|e| format!("Install task panicked: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::error::Error;
    use std::fmt;

    /// Asks the gate about a process running from `in_app_bundle` whose environment holds exactly
    /// `vars` and nothing else.
    fn skip(in_app_bundle: bool, vars: &[&str]) -> Option<SkipReason> {
        let set: HashSet<&str> = vars.iter().copied().collect();
        skip_reason_for(in_app_bundle, &|name| set.contains(name))
    }

    #[test]
    fn a_bundled_release_with_clean_env_may_check() {
        assert_eq!(skip(true, &[]), None);
    }

    #[test]
    fn an_unbundled_build_never_checks() {
        assert_eq!(skip(false, &[]), Some(SkipReason::NotAnAppBundle));
    }

    /// Every non-prod signal suppresses on its own, even from a properly bundled app. A bundled
    /// harness run is exactly the case the old `CI`-only gate let through.
    #[test]
    fn each_non_prod_env_var_suppresses_a_bundled_app() {
        for name in crate::prod_instance::NON_PROD_ENV_VARS {
            assert_eq!(
                skip(true, &[name]),
                Some(SkipReason::NonProdEnv(name)),
                "{name} alone must keep a bundled app off the update-check endpoint"
            );
        }
    }

    /// The gate and the analytics gate must agree about what a real install is, or the dashboard's
    /// `update_checks` ceiling and its heartbeat floor start counting different populations.
    #[test]
    fn every_tooling_launcher_is_suppressed_even_when_bundled() {
        // `scripts/check/checks/e2e-playwright-app.go`.
        let e2e_checker = ["CMDR_INSTANCE_ID", "CMDR_DATA_DIR", "CMDR_E2E_MODE", "CMDR_MOCK_FDA"];
        // `apps/desktop/scripts/i18n-capture.ts`.
        let i18n_capture = ["CMDR_E2E_MODE", "CMDR_DATA_DIR", "CMDR_MOCK_FDA"];
        // `apps/desktop/scripts/marketing-shots.ts` deliberately leaves `CMDR_E2E_MODE` unset.
        let marketing_shots = ["CMDR_DATA_DIR"];
        // `apps/desktop/scripts/tauri-wrapper.ts` (dev and per-worktree dev).
        let dev_wrapper = ["CMDR_INSTANCE_ID", "CMDR_DATA_DIR"];

        for (label, vars) in [
            ("e2e checker", &e2e_checker[..]),
            ("i18n capture", &i18n_capture[..]),
            ("marketing shots", &marketing_shots[..]),
            ("dev wrapper", &dev_wrapper[..]),
        ] {
            assert!(
                skip(true, vars).is_some(),
                "{label} must not reach the update-check endpoint"
            );
        }
    }

    /// The bundle condition wins, so the log names the thing that makes an update impossible
    /// rather than one that merely makes it unwanted.
    #[test]
    fn the_bundle_condition_is_reported_first() {
        assert_eq!(skip(false, &["CMDR_E2E_MODE"]), Some(SkipReason::NotAnAppBundle));
    }

    /// The log has to name the condition, or the next pollution incident is undiagnosable.
    #[test]
    fn reasons_name_the_condition() {
        assert_eq!(SkipReason::NotAnAppBundle.to_string(), "not running from a .app bundle");
        assert_eq!(
            SkipReason::NonProdEnv("CMDR_DATA_DIR").to_string(),
            "CMDR_DATA_DIR is set"
        );
    }

    #[derive(Debug)]
    struct ChainErr {
        msg: &'static str,
        source: Option<Box<dyn Error + 'static>>,
    }

    impl fmt::Display for ChainErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.msg)
        }
    }

    impl Error for ChainErr {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref()
        }
    }

    #[test]
    fn describe_error_chain_renders_only_outer_when_no_source() {
        let err = ChainErr {
            msg: "outer",
            source: None,
        };
        assert_eq!(describe_error_chain(&err), "outer");
    }

    #[test]
    fn describe_error_chain_walks_full_source_chain() {
        let inner = ChainErr {
            msg: "io broken pipe",
            source: None,
        };
        let middle = ChainErr {
            msg: "hyper transport",
            source: Some(Box::new(inner)),
        };
        let outer = ChainErr {
            msg: "reqwest send",
            source: Some(Box::new(middle)),
        };
        assert_eq!(
            describe_error_chain(&outer),
            "reqwest send: hyper transport: io broken pipe"
        );
    }

    /// Sanity-check against an actual `reqwest::Error` for a name that can never resolve
    /// (`.invalid` TLD per RFC 6761). `#[ignore]`'d because it depends on the local resolver
    /// Run with
    /// `cargo nextest run -p cmdr describe_error_chain --run-ignored=ignored-only --no-capture`
    /// to see what reqwest 0.13's source() chain actually surfaces. The `eprintln!` is
    /// allowed locally because the whole point of these tests is to render the chain into
    /// stderr for human inspection; they're verification harnesses, not production code.
    #[tokio::test]
    #[ignore = "network-dependent; run manually to verify reqwest chain content"]
    #[allow(clippy::print_stderr, reason = "verification harness; see fn doc")]
    async fn describe_error_chain_unwraps_reqwest_dns_failure() {
        let err = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .get("http://nonexistent-host-for-cmdr-tests.invalid/")
            .send()
            .await
            .expect_err("request to .invalid should fail");
        let msg = describe_error_chain(&err);
        eprintln!("DNS-failure chain: {msg}");
        assert!(msg.len() > 60, "chain too short, source() likely empty: {msg}");
    }

    /// Sanity-check against an actual connect timeout (RFC 5737 unreachable address).
    /// `#[ignore]` for the same reason as the DNS test.
    #[tokio::test]
    #[ignore = "network-dependent; run manually to verify reqwest chain content"]
    #[allow(clippy::print_stderr, reason = "verification harness; see fn doc")]
    async fn describe_error_chain_unwraps_reqwest_connect_timeout() {
        let err = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .build()
            .unwrap()
            .get("http://10.255.255.1/")
            .send()
            .await
            .expect_err("connect to 10.255.255.1 should time out");
        let msg = describe_error_chain(&err);
        eprintln!("connect-timeout chain: {msg}");
        // reqwest 0.13 wording, captured from a one-shot run on macOS:
        //   error sending request for url (http://10.255.255.1/): client error (Connect): tcp connect error: deadline has elapsed
        // Match on the "tcp connect" cause rather than a "timeout" keyword; reqwest words it
        // as "deadline has elapsed", not "timeout".
        // `#[ignore]` verification harness pinning reqwest 0.13's `source()` chain wording. Not classification; the production updater renders the chain into log strings, never branches on the words. Manual run only.
        let chain = msg.to_lowercase();
        let matched = ["tcp connect", "deadline", "timed out"]
            .iter()
            .any(|needle| chain.contains(needle));
        assert!(matched, "expected connect/deadline-shaped cause in chain: {msg}");
    }
}
