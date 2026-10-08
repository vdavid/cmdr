//! Work over many keys at once: counting what's under a folder, and deleting
//! a batch of files.
//!
//! - **`tally_subtree`** lists recursively (no delimiter), a thousand keys per
//!   request whatever the nesting, and stops once it's past the cap: F2 asks
//!   it how big a folder rename would be, and a huge folder must not be listed
//!   whole for that. Folder markers aren't files, so they don't count.
//! - **`delete_files`** sends `DeleteObjects`, a thousand keys per request with
//!   `Content-MD5`, quiet, so only failures come back, each reported against
//!   its own path. ❗ By key, with no folder check: the trait's contract is
//!   files the caller just listed (a move's source sweep, a volume delete).
//!   A throttled, faulted, or cut-off batch goes again after 1 s and 2 s: the
//!   request is idempotent.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cmdr_fs::volume::patching::patch_deleted;
use cmdr_fs::volume::{ScannedFile, SubtreeTally, VolumeError};

use super::S3Volume;
use super::errors::map_s3_error;
use super::listing::can_hold_keys;
use super::multipart_upload::retry_after;
use super::paths::{Target, target_of};
use super::query::body_error;
use crate::error::S3Error;
use crate::ops::{self, ListObjectsParams, MAX_DELETE_KEYS};
use crate::xml::{parse_delete_result, parse_list_objects};
use log::debug;

/// Adds every folder `relative` (a key under the tallied folder) sits in, or
/// names as a marker: `sub/deeper/c.txt` adds `sub` and `sub/deeper`.
fn note_folders(folders: &mut HashSet<String>, relative: &str) {
    let mut end = 0;
    while let Some(slash) = relative[end..].find('/') {
        end += slash;
        folders.insert(relative[..end].to_string());
        end += 1;
    }
}

fn unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|since| since.as_secs())
}

/// One path of a batch delete: where its result goes, its key, and the
/// server-side path its error names.
struct Doomed {
    index: usize,
    key: String,
    remote: String,
}

impl S3Volume {
    /// Counts the objects under `path` (one, for an object), stopping once
    /// more than `cap` are found.
    pub(super) async fn tally_subtree_impl(&self, path: &Path, cap: u64) -> Result<SubtreeTally, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let Target::Key { bucket, key } = target_of(&remote) else {
            // A bucket or the account isn't something a rename moves.
            return Err(VolumeError::NotSupported);
        };
        let client = self.clone_client().await?;
        let prefix = format!("{key}/");
        let mut tally = SubtreeTally {
            files: 0,
            bytes: 0,
            folders: 0,
            per_file: Vec::new(),
            complete: true,
        };
        // Every folder the keys name, relative to `prefix` ("" is the folder
        // itself): S3 has no folders, only the slashes in keys and markers.
        let mut folders: HashSet<String> = HashSet::from([String::new()]);
        // A prefix past the key ceiling holds nothing (`listing::can_hold_keys`).
        let mut more = can_hold_keys(&client.profile().normalize_key(&prefix));
        let mut found = false;
        let mut token: Option<String> = None;
        while more {
            // One past the cap is all it takes to know there's more.
            let room = cap.saturating_sub(tally.files).saturating_add(1);
            let params = ListObjectsParams {
                prefix: &prefix,
                delimiter: None,
                continuation_token: token.as_deref(),
                max_keys: Some(u32::try_from(room.min(1_000)).unwrap_or(1_000)),
            };
            let request = ops::list_objects(client.profile(), bucket, &params)
                .map_err(|_| VolumeError::NotFound(remote.clone()))?;
            let answer = self.ask(&client, request, &remote).await?;
            let page = parse_list_objects(&answer.text()).map_err(|e| body_error(&e, &remote))?;
            for object in &page.objects {
                found = true;
                let relative = object.key.strip_prefix(&prefix).unwrap_or(&object.key);
                note_folders(&mut folders, relative);
                if object.key.ends_with('/') {
                    continue;
                }
                if tally.files >= cap {
                    tally.complete = false;
                    tally.folders = folders.len() as u64;
                    return Ok(tally);
                }
                tally.files += 1;
                tally.bytes += object.size;
                tally.per_file.push(ScannedFile {
                    size: object.size,
                    modified_at: object.last_modified.and_then(unix_seconds),
                });
            }
            token = page.next_continuation_token.filter(|_| page.is_truncated);
            more = token.is_some();
        }
        if found {
            tally.folders = folders.len() as u64;
            return Ok(tally);
        }
        // Nothing under it: an object, or nothing at all.
        match self.head_object(&client, bucket, key, &remote).await? {
            Some(head) => {
                let file = ScannedFile {
                    size: head.object_length().unwrap_or(0),
                    // The upload time, ❌ never `x-amz-meta-mtime`: early
                    // deletion bills from when the object landed.
                    modified_at: head
                        .header("last-modified")
                        .and_then(|text| httpdate::parse_http_date(text).ok())
                        .and_then(unix_seconds),
                };
                Ok(SubtreeTally {
                    files: 1,
                    bytes: file.size,
                    folders: 0,
                    per_file: vec![file],
                    complete: true,
                })
            }
            None => Err(VolumeError::NotFound(remote)),
        }
    }

    /// `DeleteObjects` for `paths`, a thousand keys per request, answering one
    /// result per path in order. A key that was already gone counts as
    /// deleted (S3's rule).
    pub(super) async fn delete_files_impl(&self, paths: &[PathBuf]) -> Vec<Result<(), VolumeError>> {
        let mut results: Vec<Result<(), VolumeError>> = Vec::with_capacity(paths.len());
        // Per bucket, in first-seen order: an account-root place reaches more
        // than one.
        let mut by_bucket: Vec<(String, Vec<Doomed>)> = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            results.push(Ok(()));
            match self.to_remote_path(path) {
                Ok(remote) => match target_of(&remote) {
                    Target::Key { bucket, key } => {
                        let entry = Doomed {
                            index,
                            key: key.to_string(),
                            remote: remote.clone(),
                        };
                        match by_bucket.iter_mut().find(|(name, _)| name == bucket) {
                            Some((_, keys)) => keys.push(entry),
                            None => by_bucket.push((bucket.to_string(), vec![entry])),
                        }
                    }
                    // Deleting a bucket, or the account, isn't a file operation.
                    _ => results[index] = Err(VolumeError::NotSupported),
                },
                Err(e) => results[index] = Err(e),
            }
        }
        let client = match self.clone_client().await {
            Ok(client) => client,
            Err(e) => {
                for (_, keys) in &by_bucket {
                    for doomed in keys {
                        results[doomed.index] = Err(e.clone());
                    }
                }
                return results;
            }
        };
        for (bucket, keys) in &by_bucket {
            for batch in keys.chunks(MAX_DELETE_KEYS) {
                self.delete_batch(&client, bucket, batch, &mut results).await;
            }
        }
        for (path, result) in paths.iter().zip(&results) {
            if result.is_ok() {
                patch_deleted(self, path).await;
            }
        }
        results
    }

    /// Sends `request` (a `DeleteObjects`), and again after 1 s and 2 s while
    /// the answer is a throttle, a server fault, or a connection that failed.
    /// ❗ Safe because the request is idempotent: a key already gone answers
    /// `Deleted`. A batch of a thousand keys that failed on one blip would
    /// otherwise fail every one of them.
    async fn ask_again_on_a_blip(
        &self,
        client: &crate::transport::S3Client,
        request: crate::request::S3Request,
        path: &str,
    ) -> Result<crate::transport::Answer, VolumeError> {
        let mut attempt = 1;
        loop {
            let blip = match client.exchange(request.clone(), crate::transport::QUERY_BUDGET).await {
                Ok(answer) if answer.status.is_success() => return Ok(answer),
                Ok(answer) => {
                    let error = S3Error::from_response(answer.status, &answer.text());
                    if !(error.is_throttle() || error.is_retryable()) {
                        return Err(map_s3_error(&error, path));
                    }
                    map_s3_error(&error, path)
                }
                Err(e) => crate::transport::map_exchange_error(&e, self.volume_id(), path),
            };
            // Two more tries at most (1 s, then 2 s), then the batch fails.
            let Some(wait) = retry_after(attempt).filter(|_| attempt <= 2) else {
                return Err(blip);
            };
            debug!(target: "volume", "s3: a batch delete answered {blip:?}; again in {wait:?}");
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    /// One `DeleteObjects` request, its answer spread over `results`.
    async fn delete_batch(
        &self,
        client: &crate::transport::S3Client,
        bucket: &str,
        batch: &[Doomed],
        results: &mut [Result<(), VolumeError>],
    ) {
        let keys: Vec<&str> = batch.iter().map(|doomed| doomed.key.as_str()).collect();
        let first_remote = batch.first().map_or("", |doomed| doomed.remote.as_str());
        let outcome = match ops::delete_objects(client.profile(), bucket, &keys) {
            Ok(request) => self.ask_again_on_a_blip(client, request, first_remote).await,
            Err(_) => Err(VolumeError::NotFound(first_remote.to_string())),
        };
        let answer = match outcome {
            Ok(answer) => answer,
            Err(e) => {
                for doomed in batch {
                    results[doomed.index] = Err(e.clone());
                }
                return;
            }
        };
        // ❗ Parsed even on 200: a failure can sit inside it, per key or whole.
        let deleted = match parse_delete_result(&answer.text()) {
            Ok(deleted) => deleted,
            Err(e) => {
                let error = body_error(&e, first_remote);
                for doomed in batch {
                    results[doomed.index] = Err(error.clone());
                }
                return;
            }
        };
        // The server names a failed key as it stores it (NFC on R2).
        let by_key: HashMap<String, (usize, &str)> = batch
            .iter()
            .map(|doomed| {
                (
                    client.profile().normalize_key(&doomed.key).into_owned(),
                    (doomed.index, doomed.remote.as_str()),
                )
            })
            .collect();
        for failure in deleted.failed {
            if let Some((index, remote)) = by_key.get(&failure.key) {
                let error = S3Error {
                    status: answer.status,
                    code: failure.code,
                    message: failure.message,
                    request_id: None,
                    region: None,
                    endpoint: None,
                };
                results[*index] = Err(map_s3_error(&error, remote));
            }
        }
    }
}
