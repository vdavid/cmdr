//! Which price table the estimator reads: the server's copy
//! (`api.getcmdr.com/s3-prices/v1`) once fetched, kept on disk between runs,
//! else the one bundled in `cmdr-s3`. A price change reaches every install with
//! a Worker deploy, no release.
//!
//! ❗ Never on the estimate's path: a lookup answers from what's in hand at once,
//! and a stale table starts a background refresh that the NEXT estimate sees.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use cmdr_s3::cost::PriceTable;

use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use crate::managed_policy::Egress;
use crate::server_request::{self, ServerRequestError};

const PRICES_URL: &str = "https://api.getcmdr.com/s3-prices/v1";

/// The cache file in the app's data dir: the server's answer, verbatim.
const CACHE_FILE: &str = "s3-prices.json";

/// How old a table may get before a lookup refreshes it. Prices move a few
/// times a year; a day keeps the request count to one per install per day.
const REFRESH_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The table in use, once a lookup has loaded one.
static TABLE: LazyLock<RwLock<Option<Arc<PriceTable>>>> = LazyLock::new(|| RwLock::new(None));

/// When this run last started a refresh, so a failing server is asked at most
/// once per [`REFRESH_EVERY`], not once per dialog.
static LAST_ATTEMPT: Mutex<Option<Instant>> = Mutex::new(None);

/// The table to price with now. Loads the disk cache (or the bundled copy) on
/// first use, and starts a background refresh when it's stale.
pub(super) fn current(data_dir: Option<&Path>) -> Arc<PriceTable> {
    let cache = data_dir.map(|dir| dir.join(CACHE_FILE));
    let table = loaded(cache.as_deref());
    if let Some(cache) = cache
        && fetching_allowed()
        && needs_refresh(cache_age(&cache), last_attempt_age())
    {
        *LAST_ATTEMPT.lock_ignore_poison() = Some(Instant::now());
        tauri::async_runtime::spawn(refresh(cache));
    }
    table
}

fn loaded(cache: Option<&Path>) -> Arc<PriceTable> {
    if let Some(table) = TABLE.read_ignore_poison().as_ref() {
        return Arc::clone(table);
    }
    let table = Arc::new(cache.and_then(read_cache).unwrap_or_else(PriceTable::bundled));
    Arc::clone(TABLE.write_ignore_poison().get_or_insert(table))
}

/// The cached table, when the file is there and still parses under this build.
fn read_cache(path: &Path) -> Option<PriceTable> {
    let json = std::fs::read_to_string(path).ok()?;
    match PriceTable::parse(&json) {
        Ok(table) => Some(table),
        Err(e) => {
            log::warn!(target: "s3_costs", "ignoring the cached price table: {e}");
            None
        }
    }
}

/// Whether to ask the server: the cache is missing or older than
/// [`REFRESH_EVERY`], and this run hasn't asked within that window.
fn needs_refresh(cache_age: Option<Duration>, last_attempt_age: Option<Duration>) -> bool {
    let stale = cache_age.is_none_or(|age| age >= REFRESH_EVERY);
    let asked_lately = last_attempt_age.is_some_and(|age| age < REFRESH_EVERY);
    stale && !asked_lately
}

fn cache_age(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn last_attempt_age() -> Option<Duration> {
    LAST_ATTEMPT.lock_ignore_poison().map(|at| at.elapsed())
}

/// Cmdr's own dev, test, and capture runs price from the bundled copy: they
/// mustn't depend on the network, and a dev build would otherwise ask a
/// production server from every worktree.
fn fetching_allowed() -> bool {
    crate::prod_instance::non_prod_env_var_in(&|name| std::env::var_os(name).is_some()).is_none()
}

async fn refresh(cache: PathBuf) {
    match fetch(PRICES_URL).await {
        Ok((table, json)) => {
            if let Err(e) = write_cache(&cache, &json) {
                log::warn!(target: "s3_costs", "couldn't cache the price table at {}: {e}", cache.display());
            }
            *TABLE.write_ignore_poison() = Some(Arc::new(table));
            log::info!(target: "s3_costs", "refreshed the S3 price table");
        }
        Err(e) => log::warn!(target: "s3_costs", "kept the price table in hand: {e}"),
    }
}

/// Why a fetched table wasn't taken.
#[derive(Debug)]
enum FetchError {
    Request(ServerRequestError),
    Table(cmdr_s3::cost::PriceTableError),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(e) => write!(f, "{e}"),
            Self::Table(e) => write!(f, "{e}"),
        }
    }
}

/// The table at `url`, with its JSON for the cache. Split out so a test can
/// point it at a mock server.
async fn fetch(url: &str) -> Result<(PriceTable, String), FetchError> {
    let client = cmdr_http::client_builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| FetchError::Request(ServerRequestError::unexpected(e)))?;
    let response = server_request::send(Egress::S3PriceList, client.get(url))
        .await
        .map_err(FetchError::Request)?;
    let json = response
        .text()
        .await
        .map_err(|e| FetchError::Request(ServerRequestError::from_transport(&e)))?;
    let table = PriceTable::parse(&json).map_err(FetchError::Table)?;
    Ok((table, json))
}

/// Temp file then rename, so a crash mid-write leaves the old cache or none,
/// never half a file.
fn write_cache(path: &Path, json: &str) -> std::io::Result<()> {
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
#[path = "price_source_tests.rs"]
mod price_source_tests;
