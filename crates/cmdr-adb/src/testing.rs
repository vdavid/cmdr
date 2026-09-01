//! A fake ADB server for tests: the host protocol over a `TcpListener` on
//! loopback, backed by an in-memory device tree.
//!
//! Compiled under `#[cfg(test)]` and the `testing` feature, so the app's ADB
//! suites use the same fixture as this crate's.
//!
//! ## What it speaks
//!
//! - `host:version`, `host:devices`, `host:devices-l`.
//! - `host:track-devices`: the current short list, then one push per
//!   [`FakeAdbServer::push_devices`].
//! - `host:transport:<serial>`: `OKAY` for a listed [`AdbDeviceState::Ready`]
//!   device, `FAIL` otherwise. The socket then takes ONE device service:
//! - `host-serial:<serial>:features`: the string set by
//!   [`FakeAdbServer::set_features`] (all four this crate reads, by default).
//! - `sync:` with `STAT`/`STA2`/`LIST`/`LIS2`/`RECV`/`RCV2`/`SEND`/`SND2`/`QUIT`
//!   over the [`FakeTree`].
//! - `shell,v2,raw:<cmd>` with `mkdir [-p]`, `rmdir`, `rm [-rf]`, `mv`,
//!   `cp [-f]`, `df -k`, `readlink -f`, `test -e|-d|-f|-w` (`-w` follows
//!   [`FakeTree::read_only`]), and `stat -c '%f %s %Y'`; anything else exits
//!   127.
//!
//! ## Faults
//!
//! [`FakeAdbServer::drop_connections`] kills every open socket (a tracker sees
//! its stream end and reconnects); [`FakeAdbServer::stop`] closes the listener
//! too. [`FakeTree::read_only`] makes every write answer `EROFS`.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::devices::{AdbDevice, AdbDeviceState};
use crate::errors::{EEXIST, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY_DEVICE, EROFS};
use crate::server::AdbEndpoint;
use crate::sync::MAX_DATA_CHUNK;
use crate::transport::hex_message;

/// The serial of the one device a fresh fake lists.
pub const FAKE_SERIAL: &str = "emulator-5554";

/// What a current device advertises. Everything this crate reads is on.
pub const FAKE_FEATURES: &str = "shell_v2,cmd,stat_v2,ls_v2,fixed_push_mkdir,apex,abb,fixed_push_symlink_timestamp,abb_exec,remount_shell,track_app,sendrecv_v2,sendrecv_v2_brotli,sendrecv_v2_lz4,sendrecv_v2_zstd,sendrecv_v2_dry_run_send,openscreen_mdns";

/// The device a fresh fake lists: `FAKE_SERIAL`, ready, with the long fields.
pub fn fake_device() -> AdbDevice {
    AdbDevice {
        serial: FAKE_SERIAL.to_string(),
        state: AdbDeviceState::Ready,
        product: Some("sdk_gphone64_arm64".to_string()),
        model: Some("Fake_Phone".to_string()),
        device: Some("emu64a".to_string()),
        transport_id: Some(1),
    }
}

/// One node of the in-memory device filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeNode {
    /// A regular file.
    File {
        /// Its contents.
        data: Vec<u8>,
        /// Its full mode word (type bits included).
        mode: u32,
        /// Modification time, seconds since the epoch.
        mtime: i64,
    },
    /// A directory.
    Dir {
        /// Its full mode word.
        mode: u32,
        /// Modification time.
        mtime: i64,
    },
    /// A symlink. The sync service reports it as a link; `readlink -f`
    /// resolves it.
    Symlink {
        /// Where it points.
        target: String,
        /// Modification time.
        mtime: i64,
    },
}

impl FakeNode {
    /// The mode word the sync service reports.
    pub fn mode(&self) -> u32 {
        match self {
            Self::File { mode, .. } | Self::Dir { mode, .. } => *mode,
            Self::Symlink { .. } => 0o120777,
        }
    }

    /// The size the sync service reports.
    pub fn size(&self) -> u64 {
        match self {
            Self::File { data, .. } => data.len() as u64,
            Self::Dir { .. } => 4096,
            Self::Symlink { target, .. } => target.len() as u64,
        }
    }

    /// The mtime the sync service reports.
    pub fn mtime(&self) -> i64 {
        match self {
            Self::File { mtime, .. } | Self::Dir { mtime, .. } | Self::Symlink { mtime, .. } => *mtime,
        }
    }
}

/// The in-memory device filesystem, keyed by absolute path.
#[derive(Debug, Clone)]
pub struct FakeTree {
    nodes: BTreeMap<String, FakeNode>,
    /// What `df -k` reports as the total, in KiB.
    pub total_kib: u64,
    /// What `df -k` reports as available, in KiB.
    pub available_kib: u64,
    /// When set, every write answers `EROFS`.
    pub read_only: bool,
}

impl Default for FakeTree {
    fn default() -> Self {
        Self::new()
    }
}

/// The mtime every node gets unless a test sets one: 2026-01-01T00:00:00Z.
pub const DEFAULT_MTIME: i64 = 1_767_225_600;

impl FakeTree {
    /// A tree holding `/` and `/sdcard`.
    pub fn new() -> Self {
        let mut tree = Self {
            nodes: BTreeMap::new(),
            total_kib: 118_120_468,
            available_kib: 96_764_008,
            read_only: false,
        };
        tree.add_dir("/");
        tree.add_dir("/sdcard");
        tree
    }

    /// Normalizes a device path: leading `/`, no trailing `/` (except `/`).
    pub fn normalize(path: &str) -> String {
        let mut out = String::from("/");
        for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
            if !out.ends_with('/') {
                out.push('/');
            }
            out.push_str(part);
        }
        out
    }

    fn parent_of(path: &str) -> Option<String> {
        if path == "/" {
            return None;
        }
        let idx = path.rfind('/')?;
        Some(if idx == 0 {
            "/".to_string()
        } else {
            path[..idx].to_string()
        })
    }

    /// Adds a directory, creating ancestors. Chainable.
    pub fn add_dir(&mut self, path: &str) -> &mut Self {
        let path = Self::normalize(path);
        if let Some(parent) = Self::parent_of(&path) {
            self.add_dir(&parent);
        }
        self.nodes.entry(path).or_insert(FakeNode::Dir {
            mode: 0o040755,
            mtime: DEFAULT_MTIME,
        });
        self
    }

    /// Adds a file with `data`, creating ancestors. Chainable.
    pub fn add_file(&mut self, path: &str, data: &[u8]) -> &mut Self {
        let path = Self::normalize(path);
        if let Some(parent) = Self::parent_of(&path) {
            self.add_dir(&parent);
        }
        self.nodes.insert(
            path,
            FakeNode::File {
                data: data.to_vec(),
                mode: 0o100644,
                mtime: DEFAULT_MTIME,
            },
        );
        self
    }

    /// Adds a symlink to `target`, creating ancestors. Chainable.
    pub fn add_symlink(&mut self, path: &str, target: &str) -> &mut Self {
        let path = Self::normalize(path);
        if let Some(parent) = Self::parent_of(&path) {
            self.add_dir(&parent);
        }
        self.nodes.insert(
            path,
            FakeNode::Symlink {
                target: target.to_string(),
                mtime: DEFAULT_MTIME,
            },
        );
        self
    }

    /// The node at `path`, if any.
    pub fn get(&self, path: &str) -> Option<&FakeNode> {
        self.nodes.get(&Self::normalize(path))
    }

    /// A file's bytes, if `path` is a file.
    pub fn file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        match self.get(path) {
            Some(FakeNode::File { data, .. }) => Some(data.clone()),
            _ => None,
        }
    }

    /// Every path in the tree, sorted.
    pub fn paths(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    /// The direct children of `dir`: `(name, node)`.
    pub fn children(&self, dir: &str) -> Vec<(String, FakeNode)> {
        let dir = Self::normalize(dir);
        let prefix = if dir == "/" { "/".to_string() } else { format!("{dir}/") };
        self.nodes
            .iter()
            .filter(|(p, _)| p.starts_with(&prefix) && !p[prefix.len()..].contains('/') && p.len() > prefix.len())
            .map(|(p, n)| (p[prefix.len()..].to_string(), n.clone()))
            .collect()
    }

    /// Stat as the sync service would: `Err(errno)` when missing.
    pub fn stat(&self, path: &str) -> Result<FakeNode, i32> {
        self.get(path).cloned().ok_or(ENOENT)
    }

    /// Creates `path` and every missing ancestor (`mkdir -p`).
    pub fn mkdir_p(&mut self, path: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        match self.nodes.get(&path) {
            Some(FakeNode::Dir { .. }) => Ok(()),
            Some(_) => Err(EEXIST),
            None => {
                self.add_dir(&path);
                Ok(())
            }
        }
    }

    /// Writes a file (`SEND`). The parent must exist and be a directory.
    pub fn write_file(&mut self, path: &str, data: Vec<u8>, mode: u32, mtime: i64) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        match Self::parent_of(&path).and_then(|p| self.nodes.get(&p)) {
            Some(FakeNode::Dir { .. }) => {}
            _ => return Err(ENOENT),
        }
        if matches!(self.nodes.get(&path), Some(FakeNode::Dir { .. })) {
            return Err(EISDIR);
        }
        let mode = if mode & 0o170000 == 0 { 0o100000 | mode } else { mode };
        self.nodes.insert(path, FakeNode::File { data, mode, mtime });
        Ok(())
    }

    /// Removes `path` and everything under it (`rm -rf`). `Err(ENOENT)` when
    /// nothing was there.
    pub fn remove_tree(&mut self, path: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        if !self.nodes.contains_key(&path) {
            return Err(ENOENT);
        }
        let prefix = format!("{path}/");
        self.nodes.retain(|p, _| p != &path && !p.starts_with(&prefix));
        Ok(())
    }

    /// Removes one node, refusing a directory that still holds something.
    pub fn remove_one(&mut self, path: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        if !self.nodes.contains_key(&path) {
            return Err(ENOENT);
        }
        if !self.children(&path).is_empty() {
            return Err(ENOTEMPTY_DEVICE);
        }
        self.nodes.remove(&path);
        Ok(())
    }

    /// Renames `from` to `to` (`mv`), overwriting a file at `to`. Moves a
    /// whole subtree.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let from = Self::normalize(from);
        let to = Self::normalize(to);
        if !self.nodes.contains_key(&from) {
            return Err(ENOENT);
        }
        match Self::parent_of(&to).and_then(|p| self.nodes.get(&p)) {
            Some(FakeNode::Dir { .. }) => {}
            _ => return Err(ENOENT),
        }
        if matches!(self.nodes.get(&to), Some(FakeNode::Dir { .. })) && !self.children(&to).is_empty() {
            return Err(ENOTEMPTY_DEVICE);
        }
        let prefix = format!("{from}/");
        let moving: Vec<(String, FakeNode)> = self
            .nodes
            .iter()
            .filter(|(p, _)| *p == &from || p.starts_with(&prefix))
            .map(|(p, n)| (p.clone(), n.clone()))
            .collect();
        for (p, _) in &moving {
            self.nodes.remove(p);
        }
        for (p, n) in moving {
            let new_path = format!("{to}{}", &p[from.len()..]);
            self.nodes.insert(new_path, n);
        }
        Ok(())
    }

    /// `readlink -f`: follows a symlink at `path` (one level; relative targets
    /// resolve against its directory). A non-link answers itself.
    pub fn resolve(&self, path: &str) -> Result<String, i32> {
        let path = Self::normalize(path);
        match self.nodes.get(&path) {
            None => Err(ENOENT),
            Some(FakeNode::Symlink { target, .. }) => {
                if target.starts_with('/') {
                    Ok(Self::normalize(target))
                } else {
                    let parent = Self::parent_of(&path).unwrap_or_else(|| "/".to_string());
                    Ok(Self::normalize(&format!("{parent}/{target}")))
                }
            }
            Some(_) => Ok(path),
        }
    }
}

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
                    connections.lock().expect("connections lock").push(handle);
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
        *self.features.lock().expect("features lock") = list.to_string();
    }

    /// Kills every open socket. The listener stays up, so clients reconnect.
    pub fn drop_connections(&self) {
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *self.connections.lock().expect("connections lock"));
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
            let features = shared.features.lock().expect("features lock").clone();
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
                let body = stat_v1_bytes(shared.tree.lock().expect("tree lock").get(&path));
                stream.write_all(b"STAT").await?;
                stream.write_all(&body).await?;
            }
            b"STA2" => {
                let path = read_sync_path(stream, arg).await?;
                let body = stat_v2_bytes(&shared.tree.lock().expect("tree lock").stat(&path));
                stream.write_all(b"STA2").await?;
                stream.write_all(&body).await?;
            }
            b"LIST" | b"LIS2" => {
                let v2 = &id == b"LIS2";
                let path = read_sync_path(stream, arg).await?;
                let mut out = Vec::new();
                {
                    let tree = shared.tree.lock().expect("tree lock");
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
                    let tree = shared.tree.lock().expect("tree lock");
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

/// Splits a POSIX command line into words: single quotes literal, double quotes
/// and backslashes handled minimally.
pub fn split_argv(line: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    cur.push(q);
                }
            }
            '"' => {
                in_word = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => {
                            if let Some(n) = chars.next() {
                                cur.push(n);
                            }
                        }
                        other => cur.push(other),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    argv.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            other => {
                in_word = true;
                cur.push(other);
            }
        }
    }
    if in_word {
        argv.push(cur);
    }
    argv
}

fn errno_text(errno: i32) -> &'static str {
    match errno {
        ENOENT => "No such file or directory",
        EEXIST => "File exists",
        EISDIR => "Is a directory",
        EROFS => "Read-only file system",
        ENOTEMPTY_DEVICE => "Directory not empty",
        ENOTDIR => "Not a directory",
        _ => "Unknown error",
    }
}

/// Runs one fake shell command over the tree: `(exit_code, stdout, stderr)`.
pub fn run_fake_shell(tree: &Mutex<FakeTree>, argv: &[String]) -> (u8, String, String) {
    let Some(cmd) = argv.first() else {
        return (0, String::new(), String::new());
    };
    let flags: Vec<&str> = argv[1..]
        .iter()
        .map(String::as_str)
        .filter(|a| a.starts_with('-'))
        .collect();
    let args: Vec<&str> = argv[1..]
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let mut tree = tree.lock().expect("tree lock");
    match cmd.as_str() {
        "mkdir" => {
            for p in &args {
                let result = if flags.contains(&"-p") {
                    tree.mkdir_p(p)
                } else if tree.get(p).is_some() {
                    Err(EEXIST)
                } else {
                    tree.mkdir_p(p)
                };
                if let Err(e) = result {
                    return (1, String::new(), format!("mkdir: '{p}': {}\n", errno_text(e)));
                }
            }
            (0, String::new(), String::new())
        }
        "rm" => {
            let recursive = flags.iter().any(|f| f.contains('r'));
            let force = flags.iter().any(|f| f.contains('f'));
            for p in &args {
                let result = if recursive {
                    tree.remove_tree(p)
                } else {
                    tree.remove_one(p)
                };
                match result {
                    Ok(()) => {}
                    Err(ENOENT) if force => {}
                    Err(e) => return (1, String::new(), format!("rm: {p}: {}\n", errno_text(e))),
                }
            }
            (0, String::new(), String::new())
        }
        "mv" => {
            let [from, to] = args[..] else {
                return (1, String::new(), "mv: need two arguments\n".to_string());
            };
            match tree.rename(from, to) {
                Ok(()) => (0, String::new(), String::new()),
                Err(e) => (
                    1,
                    String::new(),
                    format!("mv: bad rename of '{from}': {}\n", errno_text(e)),
                ),
            }
        }
        "df" => {
            let used = tree.total_kib.saturating_sub(tree.available_kib);
            let pct = (used * 100).checked_div(tree.total_kib).unwrap_or(0);
            let out = format!(
                "Filesystem      1K-blocks     Used Available Use% Mounted on\n/dev/fuse       {} {} {} {}% /storage/emulated\n",
                tree.total_kib, used, tree.available_kib, pct
            );
            (0, out, String::new())
        }
        "readlink" => {
            let Some(p) = args.first() else {
                return (1, String::new(), String::new());
            };
            match tree.resolve(p) {
                Ok(target) => (0, format!("{target}\n"), String::new()),
                Err(_) => (1, String::new(), String::new()),
            }
        }
        "test" => {
            let node = args.first().and_then(|p| tree.get(p));
            let ok = match flags.first().copied().unwrap_or("-e") {
                "-e" => node.is_some(),
                "-d" => matches!(node, Some(FakeNode::Dir { .. })),
                "-f" => matches!(node, Some(FakeNode::File { .. })),
                "-w" => node.is_some() && !tree.read_only,
                _ => false,
            };
            (u8::from(!ok), String::new(), String::new())
        }
        "rmdir" => {
            for p in &args {
                let result = match tree.get(p) {
                    Some(FakeNode::Dir { .. }) => tree.remove_one(p),
                    Some(_) => Err(ENOTDIR),
                    None => Err(ENOENT),
                };
                if let Err(e) = result {
                    return (1, String::new(), format!("rmdir: '{p}': {}\n", errno_text(e)));
                }
            }
            (0, String::new(), String::new())
        }
        "cp" => {
            let [from, to] = args[..] else {
                return (1, String::new(), "cp: need two arguments\n".to_string());
            };
            let result = match tree.get(from).cloned() {
                Some(FakeNode::File { data, mode, mtime }) => tree.write_file(to, data, mode, mtime),
                Some(_) => Err(EISDIR),
                None => Err(ENOENT),
            };
            match result {
                Ok(()) => (0, String::new(), String::new()),
                Err(e) => (
                    1,
                    String::new(),
                    format!("cp: bad copy of '{from}': {}\n", errno_text(e)),
                ),
            }
        }
        "stat" => {
            let Some(p) = args.last() else {
                return (1, String::new(), String::new());
            };
            let format = args.first().filter(|_| args.len() > 1).copied().unwrap_or("%n");
            match tree.stat(p) {
                Ok(node) => {
                    let line = format
                        .replace("%f", &format!("{:x}", node.mode()))
                        .replace("%s", &node.size().to_string())
                        .replace("%Y", &node.mtime().to_string())
                        .replace("%n", p);
                    (0, format!("{line}\n"), String::new())
                }
                Err(e) => (1, String::new(), format!("stat: '{p}': {}\n", errno_text(e))),
            }
        }
        other => (
            127,
            String::new(),
            format!("/system/bin/sh: {other}: inaccessible or not found\n"),
        ),
    }
}

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
