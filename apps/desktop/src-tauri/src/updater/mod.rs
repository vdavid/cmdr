//! Custom macOS updater that preserves TCC/Full Disk Access permissions across updates.
//!
//! Instead of replacing the entire `.app` bundle (which changes its inode and causes macOS
//! to lose track of FDA grants), this updater syncs files *into* the existing bundle,
//! preserving the directory inode and `com.apple.macl` xattr.
//!
//! Three Tauri commands:
//! - `check_for_update` — fetches `latest.json`, compares versions
//! - `download_update` — downloads tarball, verifies minisign signature
//! - `install_update` — extracts and syncs into the running `.app` bundle

mod installer;
mod manifest;
mod signature;

use manifest::UpdateInfo;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::State;

// Per-call timeouts for the manifest fetch. The default `reqwest::get` client has no
// overall timeout — a stuck TCP handshake against the redirect target can hang for
// minutes before the OS gives up. These bounds keep a flaky network from looking like
// a hung app and stop the auto-error-reporter from firing on long hangs.
const MANIFEST_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MANIFEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// Per-call timeouts for the tarball download. No overall `timeout` here — a 60+ MB
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

/// Fetches `latest.json` (via the update check proxy for analytics) and returns update info
/// if a newer version is available.
///
/// Returns `None` when:
/// - The `CI` env var is set (CI guard — avoids network calls in tests)
/// - The remote version is not newer than the current version
/// - The manifest doesn't contain an entry for this platform
#[tauri::command]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    if std::env::var("CI").is_ok() {
        log::debug!("Skipping update check in CI");
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

/// Downloads the update tarball and verifies its minisign signature.
///
/// On success, stores the tarball path in `UpdateState` for `install_update` to consume.
#[tauri::command]
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
    use std::error::Error;
    use std::fmt;

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
    /// — run with
    /// `cargo nextest run -p cmdr describe_error_chain --run-ignored=ignored-only --no-capture`
    /// to see what reqwest 0.13's source() chain actually surfaces. The `eprintln!` is
    /// allowed locally because the whole point of these tests is to render the chain into
    /// stderr for human inspection — they're verification harnesses, not production code.
    #[tokio::test]
    #[ignore = "network-dependent; run manually to verify reqwest chain content"]
    #[allow(clippy::print_stderr, reason = "verification harness — see fn doc")]
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
    #[allow(clippy::print_stderr, reason = "verification harness — see fn doc")]
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
        // Match on the "tcp connect" cause rather than a "timeout" keyword — reqwest words it
        // as "deadline has elapsed", not "timeout".
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("tcp connect") || lower.contains("deadline") || lower.contains("timed out"),
            "expected connect/deadline-shaped cause in chain: {msg}"
        );
    }
}
