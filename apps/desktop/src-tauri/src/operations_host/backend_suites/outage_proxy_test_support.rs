//! A TCP proxy a test stands between a network volume and its Docker fixture,
//! so it can take the network away and give it back: the outage `#398` is
//! about, staged without touching the shared container stack.
//!
//! ❌ Never stop or pause a fixture CONTAINER for this. The stacks are leased
//! machine-wide (`scripts/check/CLAUDE.md` § fixture stacks), so a stopped
//! container fails every other worktree's suite that happens to be running.
//! A proxy of the test's own breaks only the test's own connections.
//!
//! Protocol-blind: it forwards bytes. [`OutageProxy::cut`] drops every live
//! connection (the client sees its socket close, the way a router restart
//! looks) and refuses new ones until [`OutageProxy::restore`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use cmdr_fs::ignore_poison::IgnorePoison;

pub(crate) struct OutageProxy {
    port: u16,
    shared: Arc<Shared>,
    listener: JoinHandle<()>,
}

struct Shared {
    up: AtomicBool,
    live: Mutex<Vec<JoinHandle<()>>>,
    /// Every byte the server sent the client, across every connection.
    to_client: AtomicU64,
}

impl OutageProxy {
    /// Starts forwarding from a fresh `127.0.0.1` port to `127.0.0.1:target_port`.
    pub(crate) async fn start(target_port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("binding the outage proxy to a loopback port");
        let port = listener.local_addr().expect("the proxy's own address").port();
        let shared = Arc::new(Shared {
            up: AtomicBool::new(true),
            live: Mutex::new(Vec::new()),
            to_client: AtomicU64::new(0),
        });
        let accepting = Arc::clone(&shared);
        let listener = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                // ❗ Checked and recorded under the lock `cut` takes: a dial
                // that read "up" just before a cut must land in `live` before
                // the cut drains it, or it forwards through the outage.
                let mut live = accepting.live.lock_ignore_poison();
                if !accepting.up.load(Ordering::SeqCst) {
                    // The network is down: the connection opens and closes at
                    // once, the way a dial into a dead route fails.
                    drop(client);
                    continue;
                }
                live.push(tokio::spawn(forward(client, target_port, Arc::clone(&accepting))));
            }
        });
        Self { port, shared, listener }
    }

    /// The loopback port to connect through.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// The network goes away: every live connection closes, and new ones are
    /// refused until [`restore`](Self::restore).
    pub(crate) fn cut(&self) {
        let mut live = self.shared.live.lock_ignore_poison();
        self.shared.up.store(false, Ordering::SeqCst);
        for connection in live.drain(..) {
            connection.abort();
        }
    }

    /// The network is back: new connections forward again.
    pub(crate) fn restore(&self) {
        self.shared.up.store(true, Ordering::SeqCst);
    }

    /// Every byte the server has sent through the proxy so far: a file read
    /// twice shows.
    pub(crate) fn bytes_to_client(&self) -> u64 {
        self.shared.to_client.load(Ordering::SeqCst)
    }
}

impl Drop for OutageProxy {
    fn drop(&mut self) {
        self.listener.abort();
        for connection in self.shared.live.lock_ignore_poison().drain(..) {
            connection.abort();
        }
    }
}

async fn forward(client: TcpStream, target_port: u16, shared: Arc<Shared>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let Ok(server) = TcpStream::connect(("127.0.0.1", target_port)).await else {
        return;
    };
    let (mut client_reads, mut client_writes) = client.into_split();
    let (mut server_reads, mut server_writes) = server.into_split();
    let up = tokio::io::copy(&mut client_reads, &mut server_writes);
    // Counted as it goes, so a connection `cut` aborts still counts.
    let down = async {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = server_reads.read(&mut buf).await?;
            if n == 0 {
                return Ok::<(), std::io::Error>(());
            }
            client_writes.write_all(&buf[..n]).await?;
            shared.to_client.fetch_add(n as u64, Ordering::SeqCst);
        }
    };
    // Ends when either side closes, or when `cut` aborts this task, which drops
    // both sockets.
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
}
