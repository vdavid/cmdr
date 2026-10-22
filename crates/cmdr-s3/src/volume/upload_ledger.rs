//! The record of every write Cmdr started and hasn't settled, so a later
//! connect (or the next write to the same key) can clear what it left:
//!
//! - **A multipart upload** not finished or aborted: S3 keeps its parts
//!   forever, invisible in every listing, and bills them. The sweep aborts it.
//! - **A PUT that was cut off** on a provider not trusted to refuse a short
//!   body (`ProviderProfile::refuses_short_body`): VersityGW up to v1.8.0 stores the part
//!   that arrived under the user's name, and when the link died the write's own
//!   cleanup couldn't reach the server (#522). The sweep, or the next write to
//!   that key, removes the object if it carries the PUT's token and is short.
//!
//! ❗ **Why a local record and not the server's list.** `ListMultipartUploads`
//! answers key, upload id, initiator, and time, and nothing marks an upload as
//! Cmdr's: aborting by age or prefix would also abort another tool's live
//! upload. So the sweep aborts exactly the upload ids written here, ❌ never
//! one it didn't record (`DETAILS.md` § "Unfinished uploads"). A cut-off PUT is
//! known by its token for the same reason.
//!
//! ❗ **A write running in this process is never a leftover**, even to a
//! second place of the same account that connects mid-upload: the in-flight
//! set is process-wide for that reason. ❗ And a key being written or settled
//! is CLAIMED ([`UploadLedger::claim_key`]), so a sweep's HEAD-then-delete of a
//! cut-off PUT can't land between a new write's settle and its publish.
//!
//! The log is `<state dir>/unfinished-uploads` (`VolumeHost::state_dir`), one
//! line per event: `+ <account> <bucket> <key> <upload id>` when an upload
//! starts, `+ <account> <bucket> <key> <token> <size>` when a PUT does, and
//! `- …` when either is settled, every field percent-encoded so a key holding
//! a space or a line break can't break a line. Reading leftovers rewrites it
//! to the records still open. Without a state directory the records live in
//! memory for the session. Every access takes one process-wide lock: the
//! events come at human cadence (two per write), and one lock keeps a rewrite
//! from losing an append.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use log::warn;
use percent_encoding::percent_decode_str;
use tokio::sync::Notify;

use crate::encoding::encode_component;
use crate::params::S3ConnectionParams;

/// The log's file name inside the backend's state directory.
const LOG_NAME: &str = "unfinished-uploads";

/// The account a record belongs to: the endpoint plus the key id, the identity
/// that can abort its uploads. ❗ The endpoint is part of it, so a server that
/// moves to a new address takes its records along ([`UploadLedger::move_account`]),
/// or no sweep ever asks about them again.
pub(super) fn ledger_account(params: &S3ConnectionParams) -> String {
    format!("{}#{}", params.credential_service(), params.access_key_id())
}

/// One upload Cmdr started and hasn't finished or aborted yet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct UnfinishedUpload {
    /// The account that can abort it (`S3VolumeInner::account`): the endpoint
    /// plus the access key id.
    pub account: String,
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
}

/// One PUT Cmdr sent and hasn't settled: its key may hold a cut-off body the
/// server kept, which only `token` (`x-amz-meta-cmdr-write`) and `size` tell
/// apart from the whole object or someone else's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct UnsettledPut {
    pub account: String,
    pub bucket: String,
    pub key: String,
    pub token: String,
    pub size: u64,
}

/// What the ledger holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Record {
    Upload(UnfinishedUpload),
    Put(UnsettledPut),
}

impl Record {
    fn account(&self) -> &str {
        match self {
            Self::Upload(upload) => &upload.account,
            Self::Put(put) => &put.account,
        }
    }

    #[cfg(any(test, feature = "testing"))]
    fn key(&self) -> &str {
        match self {
            Self::Upload(upload) => &upload.key,
            Self::Put(put) => &put.key,
        }
    }

    /// The same record, under `account`.
    fn moved_to(&self, account: &str) -> Self {
        let mut moved = self.clone();
        match &mut moved {
            Self::Upload(upload) => account.clone_into(&mut upload.account),
            Self::Put(put) => account.clone_into(&mut put.account),
        }
        moved
    }
}

impl From<&UnfinishedUpload> for Record {
    fn from(upload: &UnfinishedUpload) -> Self {
        Self::Upload(upload.clone())
    }
}

impl From<&UnsettledPut> for Record {
    fn from(put: &UnsettledPut) -> Self {
        Self::Put(put.clone())
    }
}

/// An object's address on one account, the unit a claim covers.
type KeyRef = (String, String, String);

/// What the whole process knows that the log doesn't.
#[derive(Default)]
struct Registry {
    /// Writes a task in this process is running right now.
    in_flight: HashSet<Record>,
    /// Records not yet settled, for this session: the whole ledger without a
    /// state directory, and a backstop beside the log with one (an append
    /// that failed still reaches this session's sweeps). Holds what a log
    /// recorded once it's been read ([`UploadLedger::load_once`]).
    open: HashSet<Record>,
    /// Logs already read into `open`.
    loaded: HashSet<PathBuf>,
    /// Keys a write or a settle holds right now ([`KeyClaim`]).
    claimed: HashSet<KeyRef>,
}

static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

/// Wakes whoever waits for a claimed key when a claim drops.
static CLAIM_RELEASED: Notify = Notify::const_new();

fn registry() -> MutexGuard<'static, Option<Registry>> {
    REGISTRY.lock_ignore_poison()
}

/// One key held for a write or a settle; dropping it frees the key.
#[derive(Debug)]
pub(super) struct KeyClaim(KeyRef);

impl Drop for KeyClaim {
    fn drop(&mut self) {
        if let Some(registry) = registry().as_mut() {
            registry.claimed.remove(&self.0);
        }
        CLAIM_RELEASED.notify_waiters();
    }
}

/// One volume's handle on the ledger: where its log lives, if anywhere.
#[derive(Debug, Clone)]
pub(super) struct UploadLedger {
    log: Option<PathBuf>,
}

impl UploadLedger {
    /// The ledger under `dir` (`VolumeHost::state_dir`), or a session-only one.
    pub(super) fn at(dir: Option<PathBuf>) -> Self {
        Self {
            log: dir.map(|dir| dir.join(LOG_NAME)),
        }
    }

    /// `record` exists on the server now (or may), and a task is running it.
    /// ❗ An upload is recorded right after `CreateMultipartUpload` answers,
    /// before any part; a PUT right before its first request goes out.
    pub(super) fn started(&self, record: impl Into<Record>) {
        let record = record.into();
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        registry.in_flight.insert(record.clone());
        registry.open.insert(record.clone());
        self.append('+', &record);
    }

    /// `record` was settled (completed, aborted, or found clear): nothing of
    /// it is left to sweep.
    pub(super) fn finished(&self, record: impl Into<Record>) {
        let record = record.into();
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        registry.in_flight.remove(&record);
        registry.open.remove(&record);
        self.append('-', &record);
    }

    /// The task running `record` ended without settling it (an abort the
    /// server never answered, a PUT whose cleanup couldn't reach it): keep the
    /// record for the next sweep, and stop counting it as in flight.
    pub(super) fn abandoned(&self, record: impl Into<Record>) {
        let record = record.into();
        if let Some(registry) = registry().as_mut() {
            registry.in_flight.remove(&record);
        }
    }

    /// The multipart uploads a sweep for `account` may abort: every open one
    /// that no task in this process is running. Rewrites the log to the
    /// records still open.
    pub(super) fn leftovers(&self, account: &str) -> Vec<UnfinishedUpload> {
        self.open_records(account)
            .into_iter()
            .filter_map(|record| match record {
                Record::Upload(upload) => Some(upload),
                Record::Put(_) => None,
            })
            .collect()
    }

    /// The PUTs a sweep for `account` may settle: every open one no task in
    /// this process is running. Rewrites the log like [`Self::leftovers`].
    pub(super) fn unsettled_puts(&self, account: &str) -> Vec<UnsettledPut> {
        self.open_records(account)
            .into_iter()
            .filter_map(|record| match record {
                Record::Put(put) => Some(put),
                Record::Upload(_) => None,
            })
            .collect()
    }

    /// The open PUTs no task is running at one key, for a write about to go
    /// there. Reads the log once per process; after that the session's record
    /// is the whole answer, so a write costs no file read.
    pub(super) fn unsettled_puts_at(&self, account: &str, bucket: &str, key: &str) -> Vec<UnsettledPut> {
        self.load_once();
        let guard = registry();
        let Some(registry) = guard.as_ref() else {
            return Vec::new();
        };
        registry
            .open
            .iter()
            .filter(|record| !registry.in_flight.contains(record))
            .filter_map(|record| match record {
                Record::Put(put) if put.account == account && put.bucket == bucket && put.key == key => {
                    Some(put.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// Holds `key` on `account` for a write, waiting while a settle or another
    /// write holds it.
    pub(super) async fn claim_key(account: &str, bucket: &str, key: &str) -> KeyClaim {
        loop {
            let released = CLAIM_RELEASED.notified();
            let mut released = std::pin::pin!(released);
            released.as_mut().enable();
            if let Some(claim) = Self::try_claim_key(account, bucket, key) {
                return claim;
            }
            released.await;
        }
    }

    /// Holds `key` on `account` if nothing else does: a sweep skips a key a
    /// write holds, since that write settles the key itself.
    pub(super) fn try_claim_key(account: &str, bucket: &str, key: &str) -> Option<KeyClaim> {
        let held = (account.to_string(), bucket.to_string(), key.to_string());
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        // ❗ `then`, ❌ never `then_some`: a claim built for a key that's taken
        // drops right here, and its `Drop` would free the holder's claim, and
        // re-lock the registry this guard holds (a deadlock).
        registry.claimed.insert(held.clone()).then(|| KeyClaim(held))
    }

    /// Every open record of `account` that no task here is running, the log's
    /// and the session's, rewriting the log to what it still holds open.
    fn open_records(&self, account: &str) -> Vec<Record> {
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        let mut open = match &self.log {
            Some(log) => {
                let recorded = replay(log);
                compact(log, &recorded);
                recorded
            }
            None => Vec::new(),
        };
        for record in &registry.open {
            if !open.contains(record) {
                open.push(record.clone());
            }
        }
        open.retain(|record| record.account() == account && !registry.in_flight.contains(record));
        open
    }

    /// Re-keys every record of `from` to `to`, the session's and the log's: the
    /// account's server moved to a new address, and a sweep only asks about
    /// the account it connects as. One log rewrite (temp and rename), so a
    /// crash leaves the records under one account or the other, ❌ never
    /// neither. Answers how many records moved.
    pub(super) fn move_account(&self, from: &str, to: &str) -> usize {
        if from == to {
            return 0;
        }
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        let mut moved: HashSet<Record> = HashSet::new();
        // In flight too, for completeness: a move waits for every operation on
        // the account, so nothing should be.
        for records in [&mut registry.open, &mut registry.in_flight] {
            let old: Vec<Record> = records.iter().filter(|r| r.account() == from).cloned().collect();
            for record in old {
                records.remove(&record);
                let renamed = record.moved_to(to);
                moved.insert(renamed.clone());
                records.insert(renamed);
            }
        }
        if let Some(log) = &self.log {
            let recorded = replay(log);
            if recorded.iter().any(|record| record.account() == from) {
                let rewritten: Vec<Record> = recorded
                    .into_iter()
                    .map(|record| {
                        if record.account() != from {
                            return record;
                        }
                        let renamed = record.moved_to(to);
                        moved.insert(renamed.clone());
                        renamed
                    })
                    .collect();
                compact(log, &rewritten);
            }
        }
        moved.len()
    }

    /// Reads this ledger's log into the session's record, once per process.
    fn load_once(&self) {
        let Some(log) = &self.log else {
            return;
        };
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        if registry.loaded.insert(log.clone()) {
            registry.open.extend(replay(log));
        }
    }

    /// Every open multipart upload of `account` whose key starts with
    /// `prefix`, in flight or not. A fixture cell asks it about its own scratch
    /// prefix: the registry is process-wide, so an unscoped question would see
    /// another cell's records of the same account.
    #[cfg(any(test, feature = "testing"))]
    pub(super) fn open_under(&self, account: &str, prefix: &str) -> Vec<UnfinishedUpload> {
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        let mut open = self.log.as_deref().map(replay).unwrap_or_default();
        for record in registry.open.iter().chain(&registry.in_flight) {
            if !open.contains(record) {
                open.push(record.clone());
            }
        }
        open.into_iter()
            .filter(|record| record.account() == account && record.key().starts_with(prefix))
            .filter_map(|record| match record {
                Record::Upload(upload) => Some(upload),
                Record::Put(_) => None,
            })
            .collect()
    }

    /// What a crash does to a record: the in-flight mark goes with the process
    /// and only the log remains.
    #[cfg(test)]
    pub(super) fn forget_in_flight(record: impl Into<Record>) {
        let record = record.into();
        if let Some(registry) = registry().as_mut() {
            registry.in_flight.remove(&record);
            registry.open.remove(&record);
        }
    }

    /// Appends one event to the log. A failure is logged and survived: the
    /// session's own record still holds it, and a lost line only costs a
    /// leftover a later launch won't know about.
    fn append(&self, op: char, record: &Record) {
        let Some(log) = &self.log else {
            return;
        };
        let written = log
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(log))
            .and_then(|mut file| file.write_all(line(op, record).as_bytes()));
        if let Err(e) = written {
            warn!(
                target: "volume",
                "s3: couldn't record a write in {}: {e}",
                LogPrivate::file_path(log)
            );
        }
    }
}

/// One log line for `record`: four fields for an upload, five for a PUT.
fn line(op: char, record: &Record) -> String {
    match record {
        Record::Upload(upload) => format!(
            "{op} {} {} {} {}\n",
            encode_component(&upload.account),
            encode_component(&upload.bucket),
            encode_component(&upload.key),
            encode_component(&upload.upload_id)
        ),
        Record::Put(put) => format!(
            "{op} {} {} {} {} {}\n",
            encode_component(&put.account),
            encode_component(&put.bucket),
            encode_component(&put.key),
            encode_component(&put.token),
            put.size
        ),
    }
}

/// The records the log holds open: every `+` without a later `-`, in the
/// order they started. A line that doesn't read is skipped.
fn replay(log: &std::path::Path) -> Vec<Record> {
    let Ok(text) = std::fs::read_to_string(log) else {
        return Vec::new();
    };
    let mut open: Vec<Record> = Vec::new();
    for raw in text.lines() {
        let Some((opens, record)) = parse_line(raw) else {
            if !raw.is_empty() {
                warn!(
                    target: "volume",
                    "s3: skipping an unreadable line in {}",
                    LogPrivate::file_path(log)
                );
            }
            continue;
        };
        if opens {
            if !open.contains(&record) {
                open.push(record);
            }
        } else {
            open.retain(|held| held != &record);
        }
    }
    open
}

/// A line as `(opens, record)`: whether it opens its record or closes it.
fn parse_line(raw: &str) -> Option<(bool, Record)> {
    let mut fields = raw.split(' ');
    let opens = match fields.next()? {
        "+" => true,
        "-" => false,
        _ => return None,
    };
    let fields: Vec<String> = fields
        .map(|field| {
            percent_decode_str(field)
                .decode_utf8()
                .ok()
                .map(|text| text.into_owned())
        })
        .collect::<Option<_>>()?;
    let record = match <[String; 4]>::try_from(fields) {
        Ok([account, bucket, key, upload_id]) => Record::Upload(UnfinishedUpload {
            account,
            bucket,
            key,
            upload_id,
        }),
        Err(fields) => {
            let [account, bucket, key, token, size] = <[String; 5]>::try_from(fields).ok()?;
            Record::Put(UnsettledPut {
                account,
                bucket,
                key,
                token,
                size: size.parse().ok()?,
            })
        }
    };
    Some((opens, record))
}

/// Rewrites the log to `open`, through a sibling and a rename so a crash
/// mid-write leaves the old log rather than half of a new one.
fn compact(log: &std::path::Path, open: &[Record]) {
    let temp = log.with_extension("tmp");
    let text: String = open.iter().map(|record| line('+', record)).collect();
    let rewritten = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, log));
    if let Err(e) = rewritten {
        warn!(target: "volume", "s3: couldn't rewrite {}: {e}", LogPrivate::file_path(log));
    }
}

#[cfg(test)]
#[path = "upload_ledger_test.rs"]
mod upload_ledger_test;
