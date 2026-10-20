//! A TCP proxy that can black-hole the connections it carries: the network drop
//! that sends the server no FIN, which is what leaves a dropped session's
//! handles open on the server for minutes.
//!
//! [`BlackHoleProxy::black_hole`] stops forwarding on every connection live at
//! that moment and keeps both of its sockets open, so neither side ever hears
//! the other close; the server holds the session as if the client were only
//! quiet. Connections dialed afterwards forward normally, which is the network
//! coming back for the reconnect.
//!
//! Protocol-blind: it forwards bytes and never reads them.

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

#[derive(Default)]
struct Shared {
    /// One trigger per connection still forwarding.
    live: Mutex<Vec<oneshot::Sender<()>>>,
    /// The sockets of every black-holed connection, held open until the proxy
    /// drops so nothing ever closes them.
    swallowed: Mutex<Vec<(TcpStream, TcpStream)>>,
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
                let (trigger, swallow) = oneshot::channel();
                accepting.live.lock_ignore_poison().push(trigger);
                tokio::spawn(forward(client, target_port, swallow, Arc::clone(&accepting)));
            }
        });
        Self { port, shared, listener }
    }

    /// The loopback port to connect through.
    pub(super) fn port(&self) -> u16 {
        self.port
    }

    /// Every connection live right now stops carrying bytes, with both of its
    /// sockets left open. New connections still forward.
    pub(super) fn black_hole(&self) {
        for trigger in self.shared.live.lock_ignore_poison().drain(..) {
            let _ = trigger.send(());
        }
    }
}

impl Drop for BlackHoleProxy {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

async fn forward(mut client: TcpStream, target_port: u16, swallow: oneshot::Receiver<()>, shared: Arc<Shared>) {
    let Ok(mut server) = TcpStream::connect(("127.0.0.1", target_port)).await else {
        return;
    };
    let swallowed = tokio::select! {
        _ = tokio::io::copy_bidirectional(&mut client, &mut server) => false,
        result = swallow => result.is_ok(),
    };
    if swallowed {
        shared.swallowed.lock_ignore_poison().push((client, server));
    }
}
