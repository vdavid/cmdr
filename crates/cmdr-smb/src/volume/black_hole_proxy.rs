//! A TCP proxy that can black-hole the connections it carries: the network drop
//! that sends the server no FIN, which is what leaves a dropped session's
//! handles open on the server for minutes.
//!
//! [`BlackHoleProxy::black_hole`] stops forwarding on every connection live at
//! that moment and keeps both of its sockets open, so neither side ever hears
//! the other close; the server holds the session as if the client were only
//! quiet. Connections dialed afterwards forward normally, which is the network
//! coming back for the reconnect, unless [`BlackHoleProxy::refuse_new`] holds
//! the network down: then a new connection is closed as soon as it arrives.
//!
//! [`BlackHoleProxy::cut`] is the drop both ends DO hear: every live connection
//! closes, which a client reads as its connection lost at once.
//!
//! Protocol-blind: it forwards bytes and never reads them.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub(super) struct BlackHoleProxy {
    port: u16,
    shared: Arc<Shared>,
    listener: JoinHandle<()>,
}

/// What happens to a live connection when the network goes.
enum Fate {
    /// It stops forwarding, with both sockets left open.
    Swallow,
    /// Both sockets close.
    Cut,
}

#[derive(Default)]
struct Shared {
    /// One trigger per connection still forwarding.
    live: Mutex<Vec<oneshot::Sender<Fate>>>,
    /// The sockets of every black-holed connection, held open until the proxy
    /// drops so nothing ever closes them.
    swallowed: Mutex<Vec<(TcpStream, TcpStream)>>,
    /// New connections are closed on arrival: the network is still down.
    refusing: AtomicBool,
    /// Connections forwarded so far.
    forwarded: AtomicUsize,
}

impl BlackHoleProxy {
    /// Starts forwarding from a fresh `127.0.0.1` port to `127.0.0.1:target_port`.
    pub(super) async fn start(target_port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("binding the black-hole proxy to a loopback port");
        let port = listener.local_addr().expect("the proxy's own address").port();
        let shared = Arc::new(Shared::default());
        let accepting = Arc::clone(&shared);
        let listener = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                if accepting.refusing.load(Ordering::Acquire) {
                    drop(client);
                    continue;
                }
                accepting.forwarded.fetch_add(1, Ordering::AcqRel);
                let (trigger, fate) = oneshot::channel();
                accepting.live.lock_ignore_poison().push(trigger);
                tokio::spawn(forward(client, target_port, fate, Arc::clone(&accepting)));
            }
        });
        Self { port, shared, listener }
    }

    /// How many connections the proxy has forwarded, refused ones not counted.
    pub(super) fn connections(&self) -> usize {
        self.shared.forwarded.load(Ordering::Acquire)
    }

    /// The loopback port to connect through.
    pub(super) fn port(&self) -> u16 {
        self.port
    }

    /// Every connection live right now stops carrying bytes, with both of its
    /// sockets left open. New connections still forward.
    pub(super) fn black_hole(&self) {
        for trigger in self.shared.live.lock_ignore_poison().drain(..) {
            let _ = trigger.send(Fate::Swallow);
        }
    }

    /// Every connection live right now closes at both ends. New connections
    /// still forward.
    pub(super) fn cut(&self) {
        for trigger in self.shared.live.lock_ignore_poison().drain(..) {
            let _ = trigger.send(Fate::Cut);
        }
    }

    /// While `refuse` holds, every new connection is closed as it arrives, so a
    /// reconnect fails the way it does with the network down. Live connections
    /// are left as they are.
    pub(super) fn refuse_new(&self, refuse: bool) {
        self.shared.refusing.store(refuse, Ordering::Release);
    }
}

impl Drop for BlackHoleProxy {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

async fn forward(mut client: TcpStream, target_port: u16, fate: oneshot::Receiver<Fate>, shared: Arc<Shared>) {
    let Ok(mut server) = TcpStream::connect(("127.0.0.1", target_port)).await else {
        return;
    };
    // A cut drops both sockets here, which closes them.
    let swallowed = tokio::select! {
        _ = tokio::io::copy_bidirectional(&mut client, &mut server) => false,
        fate = fate => matches!(fate, Ok(Fate::Swallow)),
    };
    if swallowed {
        shared.swallowed.lock_ignore_poison().push((client, server));
    }
}
