//! The real accounts a live run reaches, read from the environment: the one
//! list both the crate's own live cells (`live_support.rs`) and the app's
//! engine suites (`S3Target::live_all`) work from.
//!
//! ❗ Nothing here is reached unless `CMDR_S3_LIVE=1` and an account's variables
//! are all set, so CI and `pnpm check` never need an account. The runners that
//! export them from the secret store: `apps/desktop/test/s3-servers/live.sh`
//! (the crate's cells) and `live-engine.sh` (the app's engine flows). ❌ Never
//! print a secret: the variables reach the client and nothing else.

use std::sync::OnceLock;

use crate::params::S3Provider;

/// Every live object sits under this prefix, so the sweep can find them.
pub const LIVE_ROOT: &str = "cmdr-live/";

/// One real account this run reaches.
pub struct LiveAccount {
    /// `r2`, `hetzner`, `gcs`, `spaces`, `aws`, `b2`, `wasabi`: what each
    /// finding is printed under.
    pub name: &'static str,
    /// The preset the account is reached through.
    pub provider: S3Provider,
    /// The access key id; public, unlike the secret.
    pub key_id: String,
    pub(crate) secret: String,
    /// The bucket every cell works in, under a prefix of its own.
    pub bucket: String,
    /// A second bucket the same key reaches, for the cross-bucket cells.
    pub bucket_2: Option<String>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The accounts this run reaches: none unless `CMDR_S3_LIVE=1`, then each one
/// whose variables are all set (`CMDR_S3_LIVE_ONLY=r2,gcs` narrows it).
pub fn live_accounts() -> Vec<LiveAccount> {
    if var("CMDR_S3_LIVE").as_deref() != Some("1") {
        return Vec::new();
    }
    let only = var("CMDR_S3_LIVE_ONLY");
    let wanted = |name: &str| {
        only.as_deref()
            .is_none_or(|list| list.split(',').any(|n| n.trim() == name))
    };
    let mut accounts = Vec::new();
    let mut add = |name: &'static str, provider: Option<S3Provider>| {
        let upper = name.to_uppercase();
        let field = |suffix: &str| var(&format!("CMDR_S3_LIVE_{upper}_{suffix}"));
        if let (true, Some(provider), Some(key_id), Some(secret), Some(bucket)) = (
            wanted(name),
            provider,
            field("KEY_ID"),
            field("SECRET"),
            field("BUCKET"),
        ) {
            accounts.push(LiveAccount {
                name,
                provider,
                key_id,
                secret,
                bucket,
                bucket_2: field("BUCKET_2"),
            });
        }
    };
    add(
        "r2",
        var("CMDR_S3_LIVE_R2_ACCOUNT").map(|account_id| S3Provider::R2 { account_id }),
    );
    add(
        "hetzner",
        var("CMDR_S3_LIVE_HETZNER_LOCATION").map(|location| S3Provider::Hetzner { location }),
    );
    add("gcs", Some(S3Provider::Gcs));
    add(
        "spaces",
        var("CMDR_S3_LIVE_SPACES_REGION").map(|region| S3Provider::DigitalOcean { region }),
    );
    add(
        "aws",
        var("CMDR_S3_LIVE_AWS_REGION").map(|region| S3Provider::Aws { region }),
    );
    add(
        "b2",
        var("CMDR_S3_LIVE_B2_REGION").map(|region| S3Provider::B2 { region }),
    );
    add(
        "wasabi",
        var("CMDR_S3_LIVE_WASABI_REGION").map(|region| S3Provider::Wasabi { region }),
    );
    accounts
}

/// A token unique to this run, under [`LIVE_ROOT`].
pub fn live_run_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| uuid::Uuid::new_v4().simple().to_string()[..10].to_string())
}

/// `cmdr-live/<run>/`: everything this run writes, on every account.
pub fn live_run_root() -> String {
    format!("{LIVE_ROOT}{}/", live_run_token())
}

/// `cmdr-live/<run>/<label>/`.
pub fn live_prefix(label: &str) -> String {
    format!("{}{label}/", live_run_root())
}
