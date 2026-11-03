//! A tiny in-process S3 for cells that need a server to misbehave in one
//! precise way the Docker fixtures can't:
//!
//! - it commits a PUT whose whole body arrived and then waits `answer_after`
//!   before saying so (R2's slow answer);
//! - [`FakeS3::hang_up_after_commit`]: it commits a PUT or a
//!   `CompleteMultipartUpload` and then drops the connection unanswered (a
//!   link that dies right after the server published);
//! - [`FakeS3::keep_cut_off_bodies`]: it stores what arrived of a PUT cut off
//!   mid-body, the way VersityGW does, where S3 publishes nothing;
//! - it refuses a listing prefix past S3's 1,024-byte key ceiling with `400
//!   InvalidRequest` the way B2 does, where other servers answer an empty page;
//! - a `CopyObject` honours `x-amz-copy-source-if-match` and R2's
//!   `cf-copy-destination-if-none-match` (`412`), a PUT honours
//!   `If-None-Match: *` (`412`), and
//!   [`FakeS3::replace_after_head`] stands in for another writer replacing a
//!   source between the copy's HEAD and the copy;
//! - [`FakeS3::read_at`]: it reads request bodies at a set rate, a slow uplink
//!   a pause can land in the middle of, until [`FakeS3::read_freely`], and
//!   counts every body byte as it arrives ([`FakeS3::body_bytes`]);
//! - [`FakeS3::fault_puts`]: it answers a PUT `500 InternalError`, the way B2
//!   answers about one in several hundred, before or after storing it;
//! - [`FakeS3::taken_before_next_part`]: another writer's PUT ends a multipart
//!   upload mid-way, and its parts lose their answer to a reset (Garage);
//! - [`FakeS3::answer_gets_without_length`]: it answers an object GET with no
//!   `content-length` (the way a proxy that strips it does), optionally cut
//!   off partway, and [`FakeS3::answer_heads_without_length`] a HEAD likewise.
//!
//! It speaks path style over plain HTTP, one request per connection, and
//! knows HEAD, PUT, DELETE, `ListObjectsV2`, and the multipart calls. A cell
//! reaches it through a bucket place on R2 ([`FakeS3::volume`]): R2 refuses a
//! short body, so an overwrite goes as one PUT.
//!
//! This file holds the scripted [`World`] and the knobs a cell turns; the wire
//! side (reading a request, answering it) is `fake_s3/wire.rs`.

mod wire;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::volume::host::VolumeHost;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::S3Volume;
use crate::params::{S3ConnectionParams, S3Provider};
use crate::sigv4::Credentials;
use crate::transport::S3Client;
use wire::{answer, read_request};

const ACCOUNT: &str = "acct";
pub(super) const BUCKET: &str = "bucket";
const KEY_ID: &str = "AKIATEST";

/// One stored object: its length, ETag, and the `x-amz-meta-*` lines it was
/// written with.
#[derive(Clone)]
pub(super) struct Stored {
    pub len: usize,
    pub etag: String,
    pub meta: Vec<String>,
}

/// A multipart upload in progress: its key, the metadata its creation named,
/// and each part's length.
struct Upload {
    key: String,
    meta: Vec<String>,
    parts: BTreeMap<u32, usize>,
}

#[derive(Default)]
struct World {
    /// By decoded key.
    objects: BTreeMap<String, Stored>,
    uploads: BTreeMap<String, Upload>,
    writes: usize,
    /// Every listing prefix asked for, decoded.
    listed: Vec<String>,
    hang_up_after_commit: bool,
    hang_up_before_completing: bool,
    keep_cut_off_bodies: bool,
    /// The decoded key of every object request, in order.
    keys_seen: Vec<String>,
    /// How many more `DeleteObjects` requests answer `503 SlowDown`.
    throttle_batch_deletes: usize,
    /// Every `DeleteObjects` request that reached the store.
    batch_deletes: usize,
    /// The next HEAD of this key is answered, then the object replaced.
    replace_after_head: Option<String>,
    /// Bytes per second each connection's body is read at; `None` is as fast
    /// as they come.
    read_rate: Option<usize>,
    /// Every body byte read so far, counted as it arrives.
    body_bytes: usize,
    /// Every body-carrying PUT (a PUT or a part) that arrived whole.
    puts: usize,
    /// How many more whole `PutObject`s answer `500 InternalError`.
    fault_puts: usize,
    /// Whether a faulted `PutObject` stores the object before it answers.
    fault_puts_after_commit: bool,
    /// How an object GET is answered; `None` refuses it (`501`).
    gets: Option<GetBodies>,
    /// Whether an object HEAD leaves out `content-length`.
    heads_without_length: bool,
    /// The next `UploadPart` of this key finds another writer's object put
    /// there first, which ends its upload ([`FakeS3::taken_before_next_part`]).
    taken_before_next_part: Option<String>,
    /// Upload IDs another writer's PUT ended: each part sent to one is dropped
    /// unanswered.
    ended_uploads: Vec<String>,
    /// `ListMultipartUploads` answers wait while this reads `false`
    /// ([`FakeS3::hold_upload_listings`]).
    upload_listings_open: Option<tokio::sync::watch::Receiver<bool>>,
    /// `ListMultipartUploads` requests waiting on that hold right now.
    held_upload_listings: usize,
}

/// An object GET's answer: close-delimited, with no `content-length`, and
/// `cut_after` bytes of it sent when set. A ranged GET answers `206` naming the
/// object's full length in `content-range`.
#[derive(Clone, Copy)]
struct GetBodies {
    cut_after: Option<usize>,
}

pub(super) struct FakeS3 {
    addr: SocketAddr,
    world: Arc<Mutex<World>>,
    upload_listings_open: tokio::sync::watch::Sender<bool>,
}

impl FakeS3 {
    pub(super) async fn start(answer_after: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (upload_listings_open, listings_open) = tokio::sync::watch::channel(true);
        let world = Arc::new(Mutex::new(World {
            upload_listings_open: Some(listings_open),
            ..World::default()
        }));
        let serving = Arc::clone(&world);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let world = Arc::clone(&serving);
                tokio::spawn(async move {
                    let Some((head, body, length)) = read_request(&mut socket, &world).await else {
                        return;
                    };
                    let Some(response) = answer(&world, &head, &body, length, answer_after).await else {
                        return;
                    };
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self {
            addr,
            world,
            upload_listings_open,
        }
    }

    /// From now on, a `ListMultipartUploads` waits unanswered until
    /// [`Self::release_upload_listings`]: the abort's confirming listing on a
    /// server that's slow to answer it.
    pub(super) fn hold_upload_listings(&self) {
        self.upload_listings_open.send_replace(false);
    }

    /// Lets every held `ListMultipartUploads` answer, and the ones after it.
    pub(super) fn release_upload_listings(&self) {
        self.upload_listings_open.send_replace(true);
    }

    /// How many `ListMultipartUploads` requests the hold keeps waiting now.
    pub(super) fn held_upload_listings(&self) -> usize {
        self.world.lock_ignore_poison().held_upload_listings
    }

    /// From now on, a PUT or a completion is committed and then left
    /// unanswered: the connection drops after the server published.
    pub(super) fn hang_up_after_commit(&self) {
        self.world.lock_ignore_poison().hang_up_after_commit = true;
    }

    /// From now on, a `CompleteMultipartUpload` is dropped unanswered BEFORE it
    /// completes: the upload stays open and nothing is published.
    pub(super) fn hang_up_before_completing(&self) {
        self.world.lock_ignore_poison().hang_up_before_completing = true;
    }

    /// From now on, a PUT cut off mid-body stores what arrived.
    pub(super) fn keep_cut_off_bodies(&self) {
        self.world.lock_ignore_poison().keep_cut_off_bodies = true;
    }

    /// The next `count` `DeleteObjects` requests answer `503 SlowDown`,
    /// deleting nothing.
    pub(super) fn throttle_batch_deletes(&self, count: usize) {
        self.world.lock_ignore_poison().throttle_batch_deletes = count;
    }

    /// The next `count` whole `PutObject`s answer `500 InternalError` (B2's
    /// "internal incident"), storing nothing.
    pub(super) fn fault_puts(&self, count: usize) {
        self.world.lock_ignore_poison().fault_puts = count;
    }

    /// The next whole `PutObject` stores the object and still answers `500
    /// InternalError`: a fault after the publish.
    pub(super) fn fault_a_put_after_commit(&self) {
        let mut world = self.world.lock_ignore_poison();
        world.fault_puts = 1;
        world.fault_puts_after_commit = true;
    }

    /// From now on, an object GET answers its bytes (`x`s) with no
    /// `content-length`, and only the first `cut_after` of them when set.
    pub(super) fn answer_gets_without_length(&self, cut_after: Option<usize>) {
        self.world.lock_ignore_poison().gets = Some(GetBodies { cut_after });
    }

    /// From now on, an object HEAD answers with no `content-length`, the way a
    /// proxy that strips it does.
    pub(super) fn answer_heads_without_length(&self) {
        self.world.lock_ignore_poison().heads_without_length = true;
    }

    /// Garage's clash with another writer: just before the next `UploadPart`
    /// of `key`, another writer puts an object there, which ends the upload,
    /// and from then on every part of it loses its answer to a reset (Garage
    /// answers `NoSuchUpload` before reading the body and drops the
    /// connection, `apps/desktop/test/s3-servers/README.md`).
    pub(super) fn taken_before_next_part(&self, key: &str) {
        self.world.lock_ignore_poison().taken_before_next_part = Some(key.to_string());
    }

    /// How many `DeleteObjects` requests reached the store (throttled ones
    /// not counted).
    pub(super) fn batch_deletes(&self) -> usize {
        self.world.lock_ignore_poison().batch_deletes
    }

    /// The next HEAD of `key` answers as usual, and then another writer
    /// replaces it (a new ETag), as if between a copy's HEAD and the copy.
    pub(super) fn replace_after_head(&self, key: &str) {
        self.world.lock_ignore_poison().replace_after_head = Some(key.to_string());
    }

    /// From now on, every request body is read at `bytes_per_second` per
    /// connection.
    pub(super) fn read_at(&self, bytes_per_second: usize) {
        self.world.lock_ignore_poison().read_rate = Some(bytes_per_second);
    }

    /// From now on, every request body is read as fast as it comes, the
    /// connections already reading included.
    pub(super) fn read_freely(&self) {
        self.world.lock_ignore_poison().read_rate = None;
    }

    /// Every request body byte that has arrived so far, cut-off bodies too.
    pub(super) fn body_bytes(&self) -> usize {
        self.world.lock_ignore_poison().body_bytes
    }

    /// How many PUTs and parts arrived whole (copies not counted).
    pub(super) fn puts(&self) -> usize {
        self.world.lock_ignore_poison().puts
    }

    pub(super) fn object(&self, key: &str) -> Option<Stored> {
        self.world.lock_ignore_poison().objects.get(key).cloned()
    }

    /// [`Self::seed`] with `x-amz-meta-*` header lines (`"x-amz-meta-mtime: 1"`).
    pub(super) fn seed_with_meta(&self, key: &str, len: usize, meta: &[&str]) {
        self.world.lock_ignore_poison().objects.insert(
            key.to_string(),
            Stored {
                len,
                etag: "\"original\"".into(),
                meta: meta.iter().map(|line| (*line).to_string()).collect(),
            },
        );
    }

    pub(super) fn seed(&self, key: &str, len: usize) {
        self.world.lock_ignore_poison().objects.insert(
            key.to_string(),
            Stored {
                len,
                etag: "\"original\"".into(),
                meta: Vec::new(),
            },
        );
    }

    /// Every listing prefix the volume asked for.
    pub(super) fn listed(&self) -> Vec<String> {
        self.world.lock_ignore_poison().listed.clone()
    }

    /// How many multipart uploads are still open.
    pub(super) fn open_uploads(&self) -> usize {
        self.world.lock_ignore_poison().uploads.len()
    }

    /// Opens a multipart upload at `key` the way a `CreateMultipartUpload`
    /// would, answering its id: what a write a crash cut short leaves.
    pub(super) fn open_upload(&self, key: &str) -> String {
        let mut world = self.world.lock_ignore_poison();
        world.writes += 1;
        let id = format!("up{}", world.writes);
        world.uploads.insert(
            id.clone(),
            Upload {
                key: key.to_string(),
                meta: Vec::new(),
                parts: BTreeMap::new(),
            },
        );
        id
    }

    /// How many requests named `key` (any method, the multipart calls too).
    pub(super) fn requests_about(&self, key: &str) -> usize {
        self.world
            .lock_ignore_poison()
            .keys_seen
            .iter()
            .filter(|seen| *seen == key)
            .count()
    }

    /// A bucket place on R2, its endpoint dialing this fake over plain HTTP.
    pub(super) fn volume(&self) -> S3Volume {
        self.volume_for(S3Provider::R2 {
            account_id: ACCOUNT.into(),
        })
    }

    /// A bucket place on `provider`'s preset, its endpoint dialing this fake
    /// over plain HTTP. Path-style presets only (everyone but AWS).
    pub(super) fn volume_for(&self, provider: S3Provider) -> S3Volume {
        let params = S3ConnectionParams::new(provider, KEY_ID, Some(BUCKET)).expect("valid params");
        let mut profile = params.profile().expect("a profile");
        profile.scheme = "http".into();
        let host = profile.endpoint_host.clone();
        let client = S3Client::resolving(
            profile,
            Credentials::new(KEY_ID, "secret".to_string()),
            &[host],
            self.addr,
        );
        S3Volume::assemble("fake", "s3-fake", params, client, VolumeHost::detached())
    }
}
