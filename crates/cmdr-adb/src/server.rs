//! Where the ADB server is, and how to reach it.
//!
//! Cmdr never talks USB: the `adb` server daemon on `127.0.0.1:5037` owns the
//! devices and multiplexes clients. It's already running on any machine where
//! the user has run `adb` once; when it isn't, [`AdbEndpoint::connect`] starts
//! it through the platform binary, ONCE per process, and only for the default
//! endpoint. A fake or forwarded endpoint ([`AdbEndpoint::at`]) never starts
//! anything.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use log::{debug, info, warn};
use tokio::net::TcpStream;
use tokio::sync::OnceCell;

use crate::errors::AdbConnectError;
use crate::transport::AdbConnection;

/// The port the ADB server listens on unless `ANDROID_ADB_SERVER_PORT` says
/// otherwise.
pub const DEFAULT_PORT: u16 = 5037;

/// How long a TCP connect to the server may take. Loopback answers in
/// microseconds; a budget this long only ever trips on a wedged server.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long `adb start-server` may take to come up.
const START_SERVER_TIMEOUT: Duration = Duration::from_secs(15);

/// The one start attempt this process makes. `true` if the binary reported
/// success. A second refused connect after a start never starts again: either
/// the server is there and something else is wrong, or the binary itself can't
/// bring it up, and both are the user's to look at.
static SERVER_STARTED: OnceCell<bool> = OnceCell::const_new();

/// One ADB server to talk to.
#[derive(Debug, Clone)]
pub struct AdbEndpoint {
    addr: SocketAddr,
    /// The platform binary, when known up front. `None` on the default endpoint
    /// means "locate lazily on first refused connect".
    binary: Option<PathBuf>,
    may_start_server: bool,
}

impl AdbEndpoint {
    /// The local server on `127.0.0.1:5037` (or `$ANDROID_ADB_SERVER_PORT`),
    /// started on demand through the located binary.
    pub fn default_local() -> Self {
        let port = std::env::var("ANDROID_ADB_SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Self {
            addr: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            binary: None,
            may_start_server: true,
        }
    }

    /// A server at a specific address (tests, the fake server, a forwarded
    /// port). ❗ Never starts a server: nothing that runs here should spawn a
    /// process because a fixture went away.
    pub fn at(addr: SocketAddr) -> Self {
        Self {
            addr,
            binary: None,
            may_start_server: false,
        }
    }

    /// Where this endpoint dials.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Opens one socket to the server.
    ///
    /// On `ECONNREFUSED` with a start allowed, runs `adb -P <port> start-server`
    /// once per process and dials again. No binary anywhere →
    /// [`AdbConnectError::AdbNotInstalled`].
    pub async fn connect(&self) -> Result<AdbConnection, AdbConnectError> {
        match self.dial().await {
            Ok(conn) => Ok(conn),
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused && self.may_start_server => {
                let Some(binary) = self.binary.clone().or_else(locate_adb_binary) else {
                    return Err(AdbConnectError::AdbNotInstalled);
                };
                let port = self.addr.port();
                let started = SERVER_STARTED.get_or_init(|| start_server(binary, port)).await;
                if !started {
                    return Err(AdbConnectError::ServerUnreachable(err.to_string()));
                }
                self.dial()
                    .await
                    .map_err(|e| AdbConnectError::ServerUnreachable(e.to_string()))
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => Err(AdbConnectError::TimedOut),
            Err(err) => Err(AdbConnectError::ServerUnreachable(err.to_string())),
        }
    }

    async fn dial(&self) -> std::io::Result<AdbConnection> {
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(self.addr))
            .await
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))??;
        stream.set_nodelay(true)?;
        Ok(AdbConnection::from_stream(stream))
    }
}

/// Runs `adb -P <port> start-server` to completion. `true` on a zero exit.
async fn start_server(binary: PathBuf, port: u16) -> bool {
    info!("adb server not running; starting it via {}", binary.display());
    let run = tokio::process::Command::new(&binary)
        .arg("-P")
        .arg(port.to_string())
        .arg("start-server")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(START_SERVER_TIMEOUT, run).await {
        Ok(Ok(output)) if output.status.success() => {
            debug!("adb start-server succeeded");
            true
        }
        Ok(Ok(output)) => {
            warn!(
                "adb start-server exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            false
        }
        Ok(Err(err)) => {
            warn!("adb start-server could not run: {err}");
            false
        }
        Err(_) => {
            warn!("adb start-server did not finish within {START_SERVER_TIMEOUT:?}");
            false
        }
    }
}

/// Finds the platform `adb` binary, in this order: `$ADB`, `$PATH`,
/// `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`,
/// `~/Library/Android/sdk/platform-tools`, `/opt/homebrew/bin`,
/// `/usr/local/bin`. `None` when nothing executable turns up.
pub fn locate_adb_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("ADB").map(PathBuf::from).filter(|p| is_executable(p)) {
        return Some(explicit);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join("adb")));
    }
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = std::env::var_os(var) {
            candidates.push(PathBuf::from(root).join("platform-tools").join("adb"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Library/Android/sdk/platform-tools/adb"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/adb"));
    candidates.push(PathBuf::from("/usr/local/bin/adb"));
    candidates.into_iter().find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

#[cfg(test)]
#[path = "server_test.rs"]
mod server_test;
