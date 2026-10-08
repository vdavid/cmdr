//! A server that's SLOW is not a server that's gone, and the silence watch
//! tells them apart (`cmdr_fs::volume::liveness`).
//!
//! These cells need a server that answers one request while it holds another,
//! trickles a body a byte at a time, or goes quiet on command, so they bring
//! their own: [`FakeDav`], a few dozen lines of HTTP/1.1 in the test's process.
//! No Docker, so they run in the plain crate lane. The silence ladder is
//! shortened (`SHORT`) so they run in real time; the production-length ladder
//! is `cmdr-fs`'s `volume/liveness_test.rs` (paused clock) and `connection_drop_test.rs` (a real
//! server behind a black-holed proxy).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};
use cmdr_fs::volume::{ConnectionState, Volume, VolumeError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{WebdavVolume, connect_webdav_volume};
use crate::params::WebdavConnectionParams;
use cmdr_fs::volume::liveness::Timings;

/// Quiet for 300 ms, then two probes of 1 s: gone at 2.3 s. The probe budget
/// stays generous for a loopback answer, so a loaded machine can't fake a loss.
const SHORT: Timings = Timings {
    quiet: Duration::from_millis(300),
    probe_budget: Duration::from_secs(1),
    unanswered_limit: 2,
};

/// When the short ladder gives up: quiet plus every probe's whole budget.
const SHORT_DEADLINE: Duration = Duration::from_millis(2_300);

/// How long the slow cells keep the server busy: well past `SHORT_DEADLINE`,
/// so a watch that mistook slow for gone would have fired in the middle.
const BUSY_FOR: Duration = Duration::from_millis(3_000);

/// What the fake server does right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mode {
    /// A `Depth: 1` PROPFIND waits until this is off again.
    hold_listings: bool,
    /// `OPTIONS` (the watch's probe) gets an answer.
    answer_probes: bool,
    /// Nothing gets an answer: the black hole.
    silent: bool,
}

const SERVING: Mode = Mode {
    hold_listings: false,
    answer_probes: true,
    silent: false,
};

/// A WebDAV server with one collection and one file in it, `slow.txt`.
struct FakeDav {
    port: u16,
    mode: watch::Sender<Mode>,
    probes: Arc<AtomicUsize>,
}

impl FakeDav {
    async fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("binding the fake server");
        let port = listener.local_addr().expect("the fake server's address").port();
        let (mode, _) = watch::channel(SERVING);
        let probes = Arc::new(AtomicUsize::new(0));
        let (serving, answered) = (mode.clone(), Arc::clone(&probes));
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(answer(socket, serving.subscribe(), Arc::clone(&answered)));
            }
        });
        Self { port, mode, probes }
    }

    fn set(&self, mode: Mode) {
        self.mode.send_replace(mode);
    }

    fn probes_answered(&self) -> usize {
        self.probes.load(Ordering::SeqCst)
    }
}

/// One connection, one request, `Connection: close`.
async fn answer(mut socket: TcpStream, mut mode: watch::Receiver<Mode>, probes: Arc<AtomicUsize>) {
    let Some((method, depth)) = read_request(&mut socket).await else {
        return;
    };
    let now = *mode.borrow_and_update();
    if now.silent || (method == "OPTIONS" && !now.answer_probes) {
        // Hold the socket open and say nothing, for as long as the client waits.
        std::future::pending::<()>().await;
    }
    match (method.as_str(), depth.as_deref()) {
        ("OPTIONS", _) => {
            probes.fetch_add(1, Ordering::SeqCst);
            respond(&mut socket, "200 OK", "").await;
        }
        ("PROPFIND", Some("1")) => {
            let _ = mode.wait_for(|mode| !mode.hold_listings).await;
            respond(&mut socket, "207 Multi-Status", &multistatus(true)).await;
        }
        ("PROPFIND", _) => respond(&mut socket, "207 Multi-Status", &multistatus(false)).await,
        ("GET", _) => {
            // Ten bytes, one every 300 ms: 3 s of a body that never stops
            // moving, answered with 200 (the `Range` ignored, which the
            // backend handles).
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
        _ => respond(&mut socket, "405 Method Not Allowed", "").await,
    }
}

/// The request's method and `Depth` header, with its body read and dropped.
async fn read_request(socket: &mut TcpStream) -> Option<(String, Option<String>)> {
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
    let mut lines = head.lines();
    let method = lines.next()?.split_whitespace().next()?.to_string();
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
    Some((method, header("depth")))
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

/// A volume on the fake server, on the short ladder.
async fn connected(server: &FakeDav) -> (WebdavVolume, Arc<RecordingVolumeEvents>) {
    let base = Url::parse(&format!("http://127.0.0.1:{}/dav/", server.port)).expect("a valid URL");
    let params = WebdavConnectionParams::new(base, "ada", "/");
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .runtime(tokio::runtime::Handle::current())
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .credentials(Arc::new(InMemoryCredentials::new().with_entry(
            &params.credential_service(),
            Some("ada"),
            "ada",
            "secret",
        )))
        .build();
    let volume = connect_webdav_volume("fake", "webdav-fake", params, host, CancellationToken::new())
        .await
        .expect("the fake server answers the connect probe");
    volume.set_silence_timings(SHORT);
    (volume, events)
}

async fn names(volume: &WebdavVolume) -> Result<Vec<String>, VolumeError> {
    Ok(volume
        .list_directory(volume.root(), None)
        .await?
        .into_iter()
        .map(|entry| entry.name)
        .collect())
}

/// ❗ **A listing the server takes its time over is waited for, and the volume
/// never reads as down on the way**: the huge folder on a slow NAS.
///
/// The server sends nothing for the listing at all for `BUSY_FOR`, longer
/// than the ladder, but it answers the watch's probes on fresh connections,
/// which is what "busy, not gone" looks like from here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_a_busy_server_takes_its_time_over_is_waited_for() {
    let server = FakeDav::start().await;
    let (volume, events) = connected(&server).await;
    server.set(Mode {
        hold_listings: true,
        ..SERVING
    });

    let listing = tokio::spawn(async move {
        let listed = names(&volume).await;
        (listed, volume)
    });
    // allowed-test-sleep: the busy stretch IS the scenario; the server has to stay busy past the whole ladder.
    tokio::time::sleep(BUSY_FOR).await;
    server.set(SERVING);
    let (listed, volume) = listing.await.expect("the listing task");

    assert_eq!(listed.expect("a busy server's listing arrives"), vec!["slow.txt"]);
    assert!(
        events.transitions().is_empty(),
        "❌ never a disconnect for a server that's busy"
    );
    assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));
    assert!(
        server.probes_answered() >= 2,
        "the watch did ask, and got its answers: {} probes",
        server.probes_answered()
    );
}

/// ❗ **A body that keeps moving is never cut, even when the probe would go
/// unanswered**: bytes on the wire are the answer already, so a slow read
/// can't be mistaken for a lost server whatever the probe says.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_that_keeps_moving_is_never_cut() {
    let server = FakeDav::start().await;
    let (volume, events) = connected(&server).await;
    server.set(Mode {
        answer_probes: false,
        ..SERVING
    });
    let started = Instant::now();

    let read = volume.read_range(&volume.root().join("slow.txt"), 0, 10).await;

    assert_eq!(read.expect("a trickling body arrives whole"), b"slow bytes");
    assert!(
        started.elapsed() > SHORT_DEADLINE,
        "the read outlasted the ladder ({:?}), or it proves nothing",
        started.elapsed()
    );
    assert!(events.transitions().is_empty());
    assert_eq!(server.probes_answered(), 0, "nothing was ever quiet long enough to ask");
}

/// ❗ **A server that goes silent cuts every waiting operation at the ladder's
/// end with `DeviceDisconnected`, and the volume reports it ONCE.** The
/// production-length twin against a real server is in
/// `connection_drop_test.rs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_silent_server_cuts_every_waiting_operation_and_reports_once() {
    let server = FakeDav::start().await;
    let (volume, events) = connected(&server).await;
    volume.set_auto_reconnect(false);
    server.set(Mode {
        silent: true,
        ..SERVING
    });
    let started = Instant::now();

    let (first, second) = tokio::join!(names(&volume), names(&volume));
    let waited = started.elapsed();

    for listed in [&first, &second] {
        assert!(matches!(listed, Err(VolumeError::DeviceDisconnected(_))), "{listed:?}");
    }
    assert!(
        waited >= SHORT_DEADLINE - Duration::from_millis(100) && waited < SHORT_DEADLINE * 2,
        "cut after {waited:?}; the ladder ends at {SHORT_DEADLINE:?}"
    );
    assert_eq!(volume.connection_state(), Some(ConnectionState::Disconnected));
    assert_eq!(
        events
            .transitions()
            .into_iter()
            .map(|(_, state)| state)
            .collect::<Vec<_>>(),
        vec![VolumeConnection::Disconnected],
        "two operations found it gone, one report"
    );
}
