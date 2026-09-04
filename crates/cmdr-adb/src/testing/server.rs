//! The fake ADB server itself: a `TcpListener` on loopback speaking the host
//! protocol, the `sync:` service, and `shell,v2,raw` over the [`FakeTree`].
//!
//! Faults live here too: [`FakeAdbServer::drop_connections`] kills every open
//! socket, [`FakeAdbServer::stop`] closes the listener.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::devices::{AdbDevice, AdbDeviceState};
use crate::server::AdbEndpoint;
use crate::sync::MAX_DATA_CHUNK;
use crate::transport::hex_message;

use super::shell::{run_fake_shell, split_argv};
use super::tree::{FakeNode, FakeTree};
use super::{FAKE_FEATURES, fake_device};

/// The running fake. Stops on drop.
pub struct FakeAdbServer {
    addr: SocketAddr,
    tree: Arc<Mutex<FakeTree>>,
    devices: watch::Sender<Vec<AdbDevice>>,
    features: Arc<Mutex<String>>,
    accept: JoinHandle<()>,
    connections: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl std::fmt::Debug for FakeAdbServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeAdbServer")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

/// Everything a connection handler shares.
#[derive(Clone)]
struct Shared {
    tree: Arc<Mutex<FakeTree>>,
    devices: watch::Sender<Vec<AdbDevice>>,
    features: Arc<Mutex<String>>,
}

impl FakeAdbServer {
    /// Binds `127.0.0.1:0` and starts serving `tree`, listing [`fake_device`].
    /// Needs a tokio runtime.
    pub async fn start(tree: FakeTree) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let shared = Shared {
            tree: Arc::new(Mutex::new(tree)),
            devices: watch::Sender::new(vec![fake_device()]),
            features: Arc::new(Mutex::new(FAKE_FEATURES.to_string())),
        };
        let connections: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
        let accept = {
            let shared = shared.clone();
            let connections = connections.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        return;
                    };
                    let shared = shared.clone();
                    let handle = tokio::spawn(async move {
                        let _ = serve_connection(stream, shared).await;
                    });
                    connections.lock_ignore_poison().push(handle);
                }
            })
        };
        Self {
            addr,
            tree: shared.tree,
            devices: shared.devices,
            features: shared.features,
            accept,
            connections,
        }
    }

    /// An endpoint that dials this fake and never starts a real server.
    pub fn endpoint(&self) -> AdbEndpoint {
        AdbEndpoint::at(self.addr)
    }

    /// Where the fake listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Replaces the device list and pushes it to every `host:track-devices`
    /// subscriber.
    pub fn push_devices(&self, devices: Vec<AdbDevice>) {
        self.devices.send_replace(devices);
    }

    /// The current device list.
    pub fn devices(&self) -> Vec<AdbDevice> {
        self.devices.borrow().clone()
    }

    /// The device tree, for seeding and asserting.
    pub fn tree(&self) -> Arc<Mutex<FakeTree>> {
        self.tree.clone()
    }

    /// Changes what `host-serial:<serial>:features` answers.
    pub fn set_features(&self, list: &str) {
        *self.features.lock_ignore_poison() = list.to_string();
    }

    /// Kills every open socket. The listener stays up, so clients reconnect.
    pub fn drop_connections(&self) {
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *self.connections.lock_ignore_poison());
        for handle in handles {
            handle.abort();
        }
    }

    /// Closes the listener and every socket.
    pub fn stop(&self) {
        self.accept.abort();
        self.drop_connections();
    }
}

impl Drop for FakeAdbServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).await?;
    let len = usize::from_str_radix(std::str::from_utf8(&len).unwrap_or("0"), 16).unwrap_or(0);
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(String::from_utf8_lossy(&payload).into_owned())
}

async fn write_fail(stream: &mut TcpStream, msg: &str) -> std::io::Result<()> {
    stream.write_all(b"FAIL").await?;
    stream.write_all(&hex_message(msg.as_bytes())).await
}

fn short_list(devices: &[AdbDevice]) -> String {
    devices
        .iter()
        .map(|d| format!("{}\t{}\n", d.serial, d.state.as_word()))
        .collect()
}

fn long_list(devices: &[AdbDevice]) -> String {
    devices
        .iter()
        .map(|d| {
            let mut line = format!("{:<22} {}", d.serial, d.state.as_word());
            if let Some(p) = &d.product {
                line.push_str(&format!(" product:{p}"));
            }
            if let Some(m) = &d.model {
                line.push_str(&format!(" model:{m}"));
            }
            if let Some(v) = &d.device {
                line.push_str(&format!(" device:{v}"));
            }
            if let Some(t) = d.transport_id {
                line.push_str(&format!(" transport_id:{t}"));
            }
            line.push('\n');
            line
        })
        .collect()
}

async fn serve_connection(mut stream: TcpStream, shared: Shared) -> std::io::Result<()> {
    loop {
        let request = read_request(&mut stream).await?;
        if let Some(serial) = request.strip_prefix("host:transport:") {
            let state = shared
                .devices
                .borrow()
                .iter()
                .find(|d| d.serial == serial)
                .map(|d| d.state);
            match state {
                Some(AdbDeviceState::Ready) => stream.write_all(b"OKAY").await?,
                Some(AdbDeviceState::Unauthorized) => return write_fail(&mut stream, "device unauthorized").await,
                Some(other) => return write_fail(&mut stream, &format!("device {}", other.as_word())).await,
                None => return write_fail(&mut stream, &format!("device '{serial}' not found")).await,
            }
            continue;
        }
        if let Some(rest) = request.strip_prefix("host-serial:") {
            let Some((serial, "features")) = rest.rsplit_once(':') else {
                return write_fail(&mut stream, "unknown host-serial service").await;
            };
            if !shared.devices.borrow().iter().any(|d| d.serial == serial) {
                return write_fail(&mut stream, &format!("device '{serial}' not found")).await;
            }
            let features = shared.features.lock_ignore_poison().clone();
            stream.write_all(b"OKAY").await?;
            stream.write_all(&hex_message(features.as_bytes())).await?;
            continue;
        }
        if let Some(cmd) = request.strip_prefix("shell,v2,raw:") {
            stream.write_all(b"OKAY").await?;
            return serve_shell(&mut stream, &shared, cmd).await;
        }
        match request.as_str() {
            "host:version" => {
                stream.write_all(b"OKAY").await?;
                stream.write_all(&hex_message(b"0029")).await?;
            }
            "host:devices" => {
                let list = short_list(&shared.devices.borrow());
                stream.write_all(b"OKAY").await?;
                stream.write_all(&hex_message(list.as_bytes())).await?;
            }
            "host:devices-l" => {
                let list = long_list(&shared.devices.borrow());
                stream.write_all(b"OKAY").await?;
                stream.write_all(&hex_message(list.as_bytes())).await?;
            }
            "host:track-devices" => {
                let mut rx = shared.devices.subscribe();
                stream.write_all(b"OKAY").await?;
                loop {
                    let list = short_list(&rx.borrow_and_update());
                    stream.write_all(&hex_message(list.as_bytes())).await?;
                    if rx.changed().await.is_err() {
                        return Ok(());
                    }
                }
            }
            "sync:" => {
                stream.write_all(b"OKAY").await?;
                return serve_sync(&mut stream, &shared).await;
            }
            other => return write_fail(&mut stream, &format!("unknown host service: {other}")).await,
        }
    }
}

async fn read_sync_path(stream: &mut TcpStream, len: u32) -> std::io::Result<String> {
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn stat_v1_bytes(node: Option<&FakeNode>) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    let (mode, size, mtime) = node.map_or((0, 0, 0), |n| (n.mode(), n.size() as u32, n.mtime() as u32));
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&mtime.to_le_bytes());
    out
}

fn stat_v2_bytes(result: &Result<FakeNode, i32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(68);
    let (error, mode, size, mtime) = match result {
        Ok(n) => (0u32, n.mode(), n.size(), n.mtime()),
        Err(errno) => (*errno as u32, 0, 0, 0),
    };
    out.extend_from_slice(&error.to_le_bytes());
    out.extend_from_slice(&1u64.to_le_bytes()); // dev
    out.extend_from_slice(&1u64.to_le_bytes()); // ino
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // nlink
    out.extend_from_slice(&0u32.to_le_bytes()); // uid
    out.extend_from_slice(&0u32.to_le_bytes()); // gid
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&mtime.to_le_bytes()); // atime
    out.extend_from_slice(&mtime.to_le_bytes());
    out.extend_from_slice(&mtime.to_le_bytes()); // ctime
    out
}

fn sync_fail(msg: &str) -> Vec<u8> {
    let mut out = b"FAIL".to_vec();
    out.extend_from_slice(&(msg.len() as u32).to_le_bytes());
    out.extend_from_slice(msg.as_bytes());
    out
}

async fn serve_sync(stream: &mut TcpStream, shared: &Shared) -> std::io::Result<()> {
    loop {
        let mut id = [0u8; 4];
        stream.read_exact(&mut id).await?;
        let mut arg = [0u8; 4];
        stream.read_exact(&mut arg).await?;
        let arg = u32::from_le_bytes(arg);
        match &id {
            b"QUIT" => return Ok(()),
            b"STAT" => {
                let path = read_sync_path(stream, arg).await?;
                let body = stat_v1_bytes(shared.tree.lock_ignore_poison().get(&path));
                stream.write_all(b"STAT").await?;
                stream.write_all(&body).await?;
            }
            b"STA2" => {
                let path = read_sync_path(stream, arg).await?;
                let body = stat_v2_bytes(&shared.tree.lock_ignore_poison().stat(&path));
                stream.write_all(b"STA2").await?;
                stream.write_all(&body).await?;
            }
            b"LIST" | b"LIS2" => {
                let v2 = &id == b"LIS2";
                let path = read_sync_path(stream, arg).await?;
                let mut out = Vec::new();
                {
                    let tree = shared.tree.lock_ignore_poison();
                    if let Some(FakeNode::Dir { .. }) = tree.get(&path) {
                        let mut entries = vec![
                            (".".to_string(), tree.stat(&path)),
                            ("..".to_string(), tree.stat(&path)),
                        ];
                        entries.extend(tree.children(&path).into_iter().map(|(n, node)| (n, Ok(node))));
                        for (name, node) in entries {
                            if v2 {
                                out.extend_from_slice(b"DNT2");
                                out.extend_from_slice(&stat_v2_bytes(&node));
                            } else {
                                out.extend_from_slice(b"DENT");
                                out.extend_from_slice(&stat_v1_bytes(node.as_ref().ok()));
                            }
                            out.extend_from_slice(&(name.len() as u32).to_le_bytes());
                            out.extend_from_slice(name.as_bytes());
                        }
                    }
                }
                out.extend_from_slice(b"DONE");
                out.extend_from_slice(&vec![0u8; if v2 { 72 } else { 16 }]);
                stream.write_all(&out).await?;
            }
            b"RECV" | b"RCV2" => {
                let path = read_sync_path(stream, arg).await?;
                if &id == b"RCV2" {
                    let mut setup = [0u8; 8];
                    stream.read_exact(&mut setup).await?;
                }
                let data = {
                    let tree = shared.tree.lock_ignore_poison();
                    match tree.get(&path) {
                        Some(FakeNode::File { data, .. }) => Ok(data.clone()),
                        Some(_) => Err(format!("open failed: {path}: Is a directory")),
                        None => Err(format!("open failed: {path}: No such file or directory")),
                    }
                };
                match data {
                    Ok(data) => {
                        let mut out = Vec::with_capacity(data.len() + 64);
                        for chunk in data.chunks(MAX_DATA_CHUNK) {
                            out.extend_from_slice(b"DATA");
                            out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
                            out.extend_from_slice(chunk);
                        }
                        out.extend_from_slice(b"DONE");
                        out.extend_from_slice(&0u32.to_le_bytes());
                        stream.write_all(&out).await?;
                    }
                    Err(msg) => stream.write_all(&sync_fail(&msg)).await?,
                }
            }
            b"SEND" | b"SND2" => {
                let spec = read_sync_path(stream, arg).await?;
                let (path, mode) = if &id == b"SND2" {
                    let mut setup = [0u8; 12];
                    stream.read_exact(&mut setup).await?;
                    (spec, u32::from_le_bytes([setup[4], setup[5], setup[6], setup[7]]))
                } else {
                    let (p, m) = spec.rsplit_once(',').unwrap_or((&spec, "33188"));
                    (p.to_string(), m.parse().unwrap_or(0o100644))
                };
                let mut data = Vec::new();
                loop {
                    let mut id = [0u8; 4];
                    stream.read_exact(&mut id).await?;
                    let mut arg = [0u8; 4];
                    stream.read_exact(&mut arg).await?;
                    let arg = u32::from_le_bytes(arg);
                    match &id {
                        b"DATA" => {
                            let mut chunk = vec![0u8; arg as usize];
                            stream.read_exact(&mut chunk).await?;
                            data.extend_from_slice(&chunk);
                        }
                        b"DONE" => {
                            let result =
                                shared
                                    .tree
                                    .lock()
                                    .expect("tree lock")
                                    .write_file(&path, data, mode, i64::from(arg));
                            match result {
                                Ok(()) => {
                                    stream.write_all(b"OKAY").await?;
                                    stream.write_all(&0u32.to_le_bytes()).await?;
                                }
                                Err(errno) => {
                                    stream
                                        .write_all(&sync_fail(&format!("couldn't create file: errno {errno}")))
                                        .await?;
                                }
                            }
                            break;
                        }
                        _ => return Ok(()),
                    }
                }
            }
            _ => {
                stream.write_all(&sync_fail("unknown command")).await?;
                return Ok(());
            }
        }
    }
}

/// One `shell,v2` frame: a one-byte stream id, a little-endian length, the payload.
fn shell_frame(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(id);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

async fn serve_shell(stream: &mut TcpStream, shared: &Shared, cmd: &str) -> std::io::Result<()> {
    let argv = split_argv(cmd);
    let (exit_code, stdout, stderr) = run_fake_shell(&shared.tree, &argv);
    let mut out = Vec::new();
    if !stdout.is_empty() {
        out.extend_from_slice(&shell_frame(1, stdout.as_bytes()));
    }
    if !stderr.is_empty() {
        out.extend_from_slice(&shell_frame(2, stderr.as_bytes()));
    }
    out.extend_from_slice(&shell_frame(3, &[exit_code]));
    stream.write_all(&out).await?;
    stream.shutdown().await
}
