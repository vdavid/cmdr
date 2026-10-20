//! [`FakeDav`]: a few dozen lines of HTTP/1.1 in the test's process, for cells
//! that need a server to misbehave in a way the Docker fixtures can't. No
//! Docker, so its cells run in the plain crate lane.
//!
//! It answers one request per connection (`Connection: close`) and serves one
//! collection, `/dav/`, with these files:
//!
//! - `slow.txt`: ten bytes, one every 300 ms, answered `200` (the `Range`
//!   ignored, which the backend handles). The slow body `slow_server_test.rs`
//!   is about.
//! - `nolength.txt`: ten bytes with no `Content-Length`, the way a proxy that
//!   strips it answers; a ranged GET answers `206` naming the full length in
//!   `Content-Range`.
//! - `cut.txt`: as `nolength.txt`, cut off after three bytes.
//!
//! [`Mode`] switches what it does right now: hold listings, ignore the silence
//! watch's probes, or go silent altogether.
//!
//! Under `/dav/tree/` it is a small writable server instead ([`Tree`]):
//! PROPFIND, MKCOL, PUT, DELETE (recursive, as the protocol's is), and MOVE
//! with `Overwrite`. A cell scripts ANOTHER client into it with
//! [`FakeDav::after`]: something that lands the moment a given request has been
//! answered, which is the race a Docker server can't be timed into.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::{CredentialStore, InMemoryCredentials};
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeEventSink};
use cmdr_fs::volume::liveness::Timings;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{WebdavVolume, connect_webdav_volume};
use crate::params::WebdavConnectionParams;

/// Quiet for 300 ms, then two probes of 1 s: gone at 2.3 s. The probe budget
/// stays generous for a loopback answer, so a loaded machine can't fake a loss.
pub(super) const SHORT: Timings = Timings {
    quiet: Duration::from_millis(300),
    probe_budget: Duration::from_secs(1),
    unanswered_limit: 2,
};

/// The length of every file the fake serves.
pub(super) const FILE_LEN: usize = 10;

/// What the fake server does right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Mode {
    /// A `Depth: 1` PROPFIND waits until this is off again.
    pub hold_listings: bool,
    /// `OPTIONS` (the watch's probe) gets an answer.
    pub answer_probes: bool,
    /// Nothing gets an answer: the black hole.
    pub silent: bool,
}

pub(super) const SERVING: Mode = Mode {
    hold_listings: false,
    answer_probes: true,
    silent: false,
};

/// A WebDAV server with one collection and the files the module doc lists.
pub(super) struct FakeDav {
    port: u16,
    mode: watch::Sender<Mode>,
    probes: Arc<AtomicUsize>,
    tree: Arc<Mutex<Tree>>,
}

impl FakeDav {
    pub(super) async fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("binding the fake server");
        let port = listener.local_addr().expect("the fake server's address").port();
        let (mode, _) = watch::channel(SERVING);
        let probes = Arc::new(AtomicUsize::new(0));
        let tree = Arc::new(Mutex::new(Tree::default()));
        let (serving, answered, shared) = (mode.clone(), Arc::clone(&probes), Arc::clone(&tree));
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let served = Served {
                    mode: serving.subscribe(),
                    probes: Arc::clone(&answered),
                    tree: Arc::clone(&shared),
                };
                tokio::spawn(answer(socket, served));
            }
        });
        Self {
            port,
            mode,
            probes,
            tree,
        }
    }

    pub(super) fn set(&self, mode: Mode) {
        self.mode.send_replace(mode);
    }

    /// Puts a node at `rel` under `/dav/tree/`, a collection when `collection`.
    pub(super) fn put_node(&self, rel: &str, collection: bool) {
        self.tree().nodes.insert(tree_key(rel), collection);
    }

    /// Every node under `/dav/tree/`, as `rel` paths, collections with a
    /// trailing slash.
    pub(super) fn nodes(&self) -> Vec<String> {
        self.tree()
            .nodes
            .iter()
            .filter_map(|(key, &collection)| {
                let rel = key.strip_prefix(TREE)?.strip_prefix('/')?;
                Some(if collection { format!("{rel}/") } else { rel.to_string() })
            })
            .collect()
    }

    /// Another client's doing: right after the server answers the next
    /// `method` at `rel`, each of `lands` appears (`(rel, collection)`). Once.
    pub(super) fn after(&self, method: &str, rel: &str, lands: &[(&str, bool)]) {
        let lands = lands
            .iter()
            .map(|(path, collection)| (tree_key(path), *collection))
            .collect();
        self.tree().after.push(Landing {
            method: method.to_string(),
            at: tree_key(rel),
            lands,
        });
    }

    /// Every MOVE under `/dav/tree/` whose destination contains `onto` is
    /// answered `status` from now on, the way a server that won't rename a
    /// folder answers.
    pub(super) fn refuse_moves_onto(&self, onto: &'static str, status: &'static str) {
        self.tree().refuse_moves = Some((onto, status));
    }

    fn tree(&self) -> std::sync::MutexGuard<'_, Tree> {
        self.tree
            .lock()
            .expect("fake tree poisoned: a cell panicked holding it")
    }

    pub(super) fn probes_answered(&self) -> usize {
        self.probes.load(Ordering::SeqCst)
    }

    /// The store key the fake's volumes file account `ada` under.
    pub(super) fn credential_service(&self) -> String {
        self.params().credential_service()
    }

    fn params(&self) -> WebdavConnectionParams {
        let base = Url::parse(&format!("http://127.0.0.1:{}/dav/", self.port)).expect("a valid URL");
        WebdavConnectionParams::new(base, "ada", "/")
    }

    /// A volume on the fake server, on the short ladder.
    pub(super) async fn connected(&self) -> (WebdavVolume, Arc<RecordingVolumeEvents>) {
        let store = InMemoryCredentials::new().with_entry(&self.credential_service(), Some("ada"), "ada", "secret");
        self.connected_with(Arc::new(store)).await
    }

    /// The same, over a secret store the caller holds on to. The fake takes any
    /// password, so whatever a sign-in typed connects.
    pub(super) async fn connected_with(
        &self,
        credentials: Arc<dyn CredentialStore>,
    ) -> (WebdavVolume, Arc<RecordingVolumeEvents>) {
        let params = self.params();
        let events = Arc::new(RecordingVolumeEvents::new());
        let host = VolumeHost::builder()
            .runtime(tokio::runtime::Handle::current())
            .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
            .credentials(credentials)
            .build();
        let volume = connect_webdav_volume("fake", "webdav-fake", params, host, CancellationToken::new())
            .await
            .expect("the fake server answers the connect probe");
        volume.set_silence_timings(SHORT);
        (volume, events)
    }
}

/// One request's line and the headers the fake reads.
struct Request {
    method: String,
    path: String,
    depth: Option<String>,
    range_start: Option<usize>,
    destination: Option<String>,
    overwrite: Option<String>,
}

/// What one connection's handler shares with the server.
struct Served {
    mode: watch::Receiver<Mode>,
    probes: Arc<AtomicUsize>,
    tree: Arc<Mutex<Tree>>,
}

/// One connection, one request, `Connection: close`.
async fn answer(mut socket: TcpStream, served: Served) {
    let Served { mut mode, probes, tree } = served;
    let Some(request) = read_request(&mut socket).await else {
        return;
    };
    let now = *mode.borrow_and_update();
    if now.silent || (request.method == "OPTIONS" && !now.answer_probes) {
        // Hold the socket open and say nothing, for as long as the client waits.
        std::future::pending::<()>().await;
    }
    if decoded(&request.path).starts_with(TREE) {
        let (status, body) = tree
            .lock()
            .expect("fake tree poisoned: a cell panicked holding it")
            .answer(&request);
        respond(&mut socket, status, &body).await;
        return;
    }
    match (request.method.as_str(), request.depth.as_deref()) {
        ("OPTIONS", _) => {
            probes.fetch_add(1, Ordering::SeqCst);
            respond(&mut socket, "200 OK", "").await;
        }
        ("PROPFIND", Some("1")) => {
            let _ = mode.wait_for(|mode| !mode.hold_listings).await;
            respond(&mut socket, "207 Multi-Status", &multistatus(true)).await;
        }
        ("PROPFIND", _) => respond(&mut socket, "207 Multi-Status", &multistatus(false)).await,
        ("GET", _) => match request.path.as_str() {
            "/dav/slow.txt" => trickle(&mut socket).await,
            "/dav/nolength.txt" => without_length(&mut socket, request.range_start, FILE_LEN).await,
            "/dav/cut.txt" => without_length(&mut socket, request.range_start, 3).await,
            _ => respond(&mut socket, "404 Not Found", "").await,
        },
        _ => respond(&mut socket, "405 Method Not Allowed", "").await,
    }
}

/// `slow.txt`: ten bytes, one every 300 ms, so 3 s of a body that never stops
/// moving.
async fn trickle(socket: &mut TcpStream) {
    let head = "HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n";
    let _ = socket.write_all(head.as_bytes()).await;
    for byte in b"slow bytes" {
        // allowed-test-sleep: this IS the slow server; its pace is what the cell is about.
        tokio::time::sleep(Duration::from_millis(300)).await;
        if socket.write_all(&[*byte]).await.is_err() {
            return;
        }
    }
}

/// A `FILE_LEN`-byte file answered close-delimited with no `Content-Length`,
/// from `range_start` when the GET asked for a range, and no more than
/// `send_at_most` bytes of it.
async fn without_length(socket: &mut TcpStream, range_start: Option<usize>, send_at_most: usize) {
    let (status, content_range, from) = match range_start {
        Some(start) => (
            "206 Partial Content",
            format!("Content-Range: bytes {start}-{}/{FILE_LEN}\r\n", FILE_LEN - 1),
            start,
        ),
        None => ("200 OK", String::new(), 0),
    };
    let body = "x".repeat(FILE_LEN.saturating_sub(from).min(send_at_most));
    let reply = format!("HTTP/1.1 {status}\r\n{content_range}Connection: close\r\n\r\n{body}");
    let _ = socket.write_all(reply.as_bytes()).await;
    let _ = socket.shutdown().await;
}

/// The request's line and the headers the fake reads, with its body read and
/// dropped.
async fn read_request(socket: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
        let read = socket.read(&mut chunk).await.ok().filter(|&read| read > 0)?;
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut line = head.lines().next()?.split_whitespace();
    let method = line.next()?.to_string();
    let path = line.next()?.to_string();
    let header = |name: &str| {
        head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim().to_string())
        })
    };
    let length: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    while buffer.len() < head_end + length {
        let read = socket.read(&mut chunk).await.ok().filter(|&read| read > 0)?;
        buffer.extend_from_slice(&chunk[..read]);
    }
    let range_start = header("range").and_then(|range| range.strip_prefix("bytes=")?.split_once('-')?.0.parse().ok());
    Some(Request {
        method,
        path,
        depth: header("depth"),
        range_start,
        destination: header("destination"),
        overwrite: header("overwrite"),
    })
}

async fn respond(socket: &mut TcpStream, status: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = socket.write_all(reply.as_bytes()).await;
    let _ = socket.shutdown().await;
}

fn multistatus(with_children: bool) -> String {
    let child = if with_children {
        "<D:response><D:href>/dav/slow.txt</D:href><D:propstat><D:prop><D:resourcetype/>\
         <D:getcontentlength>10</D:getcontentlength></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\">\
         <D:response><D:href>/dav/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype>\
         </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>{child}</D:multistatus>"
    )
}

/// Where the writable tree lives, decoded and with no trailing slash.
const TREE: &str = "/dav/tree";

/// `rel` under the tree, as a node key.
fn tree_key(rel: &str) -> String {
    format!("{TREE}/{}", rel.trim_matches('/'))
}

/// A request path as a node key: percent-decoded, trailing slash dropped.
fn decoded(path: &str) -> String {
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .trim_end_matches('/')
        .to_string()
}

/// The writable tree: what's on the "disk", and what another client does to it.
struct Tree {
    /// Every node by key, `true` for a collection.
    nodes: BTreeMap<String, bool>,
    /// What lands after which request, each once.
    after: Vec<Landing>,
    /// `(onto, status)`: a MOVE whose destination contains `onto` is answered
    /// `status` instead.
    refuse_moves: Option<(&'static str, &'static str)>,
}

/// Once the next `method` at the node `at` is answered, `lands` appear
/// (`(key, collection)`).
struct Landing {
    method: String,
    at: String,
    lands: Vec<(String, bool)>,
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::from([(TREE.to_string(), true)]),
            after: Vec::new(),
            refuse_moves: None,
        }
    }
}

impl Tree {
    /// Answers `request`, then lets whatever was scripted to land after it land.
    fn answer(&mut self, request: &Request) -> (&'static str, String) {
        let at = decoded(&request.path);
        let answered = match request.method.as_str() {
            "PROPFIND" => self.propfind(&at, request.depth.as_deref() == Some("1")),
            "MKCOL" if self.nodes.contains_key(&at) => ("405 Method Not Allowed", String::new()),
            "MKCOL" | "PUT" if !self.nodes.contains_key(parent(&at)) => ("409 Conflict", String::new()),
            "MKCOL" | "PUT" => {
                self.nodes.insert(at.clone(), request.method == "MKCOL");
                ("201 Created", String::new())
            }
            "DELETE" if self.take(&at).is_empty() => ("404 Not Found", String::new()),
            "DELETE" => ("204 No Content", String::new()),
            "MOVE" => (self.move_node(&at, request), String::new()),
            _ => ("405 Method Not Allowed", String::new()),
        };
        if let Some(index) = self
            .after
            .iter()
            .position(|landing| landing.method == request.method && landing.at == at)
        {
            self.nodes.extend(self.after.remove(index).lands);
        }
        answered
    }

    fn propfind(&self, at: &str, with_children: bool) -> (&'static str, String) {
        let Some(&collection) = self.nodes.get(at) else {
            return ("404 Not Found", String::new());
        };
        let mut rows = vec![row(at, collection)];
        if collection && with_children {
            let inside = format!("{at}/");
            rows.extend(
                self.nodes
                    .iter()
                    .filter(|(key, _)| key.strip_prefix(&inside).is_some_and(|rest| !rest.contains('/')))
                    .map(|(key, &collection)| row(key, collection)),
            );
        }
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\">{}</D:multistatus>",
            rows.concat()
        );
        ("207 Multi-Status", body)
    }

    /// Removes `at` and everything under it, and hands them back.
    fn take(&mut self, at: &str) -> Vec<(String, bool)> {
        let inside = format!("{at}/");
        let keys: Vec<String> = self
            .nodes
            .keys()
            .filter(|key| *key == at || key.starts_with(&inside))
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|key| self.nodes.remove(&key).map(|collection| (key, collection)))
            .collect()
    }

    fn move_node(&mut self, at: &str, request: &Request) -> &'static str {
        let Some(to) = request
            .destination
            .as_deref()
            .and_then(|destination| Url::parse(destination).ok())
            .map(|url| decoded(url.path()))
        else {
            return "400 Bad Request";
        };
        if let Some((onto, status)) = self.refuse_moves
            && to.contains(onto)
        {
            return status;
        }
        if !self.nodes.contains_key(at) {
            return "404 Not Found";
        }
        if !self.nodes.contains_key(parent(&to)) {
            return "409 Conflict";
        }
        let replaced = self.nodes.contains_key(&to);
        if replaced {
            if request.overwrite.as_deref() == Some("F") {
                return "412 Precondition Failed";
            }
            self.take(&to);
        }
        for (key, collection) in self.take(at) {
            self.nodes.insert(format!("{to}{}", &key[at.len()..]), collection);
        }
        if replaced { "204 No Content" } else { "201 Created" }
    }
}

fn parent(key: &str) -> &str {
    key.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// One `response` element for the node at `key`, its href percent-encoded the
/// way a server sends it.
fn row(key: &str, collection: bool) -> String {
    let href: Vec<String> = key
        .split('/')
        .map(|segment| percent_encoding::utf8_percent_encode(segment, percent_encoding::NON_ALPHANUMERIC).to_string())
        .collect();
    let (slash, kind) = if collection {
        ("/", "<D:resourcetype><D:collection/></D:resourcetype>")
    } else {
        ("", "<D:resourcetype/><D:getcontentlength>0</D:getcontentlength>")
    };
    format!(
        "<D:response><D:href>{}{slash}</D:href><D:propstat><D:prop>{kind}</D:prop>\
         <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",
        href.join("/")
    )
}
