//! Atomic file IO for the Cmdr MCP server: the port file (`mcp.port`), the bearer-token
//! file (`mcp.token`), and a sibling port file for the tauri-MCP bridge plugin. External
//! readers (the `scripts/mcp-call.sh` CLI, E2E fixtures, agent helpers) discover the actual
//! bound port and token by reading `<data_dir>/<name>` rather than guessing. The in-process
//! FE keeps using the `get_mcp_port` / `get_mcp_token` IPC (same in-process state). All
//! files are written 0o600 (owner-only): the token is a secret, and an attacker who can
//! read it already has the user's filesystem access. See `docs/tooling/instance-isolation.md`
//! § "Per-resource breakdown" (Cmdr MCP HTTP port row) for the full contract.
//!
//! Write protocol:
//!   1. Write the ASCII port + `\n` to `<dir>/<name>.tmp.<pid>`.
//!   2. `fsync` so the bytes hit the disk before the rename.
//!   3. `rename` to `<dir>/<name>` (POSIX-atomic on the same filesystem).
//!
//! Read protocol:
//!   - Single read. Callers that want to wait for the file to appear poll externally
//!     (Node + bash helpers do their own 50 ms / 5 s loops). The file is created via
//!     atomic rename, so a zero-byte read is impossible: either the file isn't there yet
//!     (`NotFound`) or it has the full content.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

/// Errors that can surface from the port file. Typed so call sites can branch without
/// substring matching the message (AGENTS.md "no string-matching error classification").
///
/// Production ships only the writer: the FE reads the live port via the `get_mcp_port`
/// IPC, and external readers (`scripts/mcp-call.sh`, E2E fixtures) parse the file in
/// shell. The Rust reader ([`read_port_file`]) and its `InvalidContent` variant are
/// `#[cfg(test)]` — they verify the write path in unit tests, not in the shipping binary.
#[derive(Debug)]
pub enum PortDiscoveryError {
    /// The port file doesn't exist yet (or anymore).
    NotFound,
    /// Filesystem error reading or writing the file.
    Io(std::io::Error),
    /// The file exists but its contents don't parse as `u16` + optional newline.
    #[cfg(test)]
    InvalidContent(String),
}

impl std::fmt::Display for PortDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "port file not found"),
            Self::Io(err) => write!(f, "port file IO error: {err}"),
            #[cfg(test)]
            Self::InvalidContent(s) => write!(f, "port file content not a valid u16: {s:?}"),
        }
    }
}

impl std::error::Error for PortDiscoveryError {}

impl From<std::io::Error> for PortDiscoveryError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound
        } else {
            Self::Io(err)
        }
    }
}

/// Build the canonical port-file path. `name` is "mcp.port" or "tauri-mcp.port".
pub fn port_file_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

/// Create a tempfile for the atomic write with owner-only perms baked in (0o600 on unix,
/// no umask window). The atomic rename preserves the tempfile's mode, so the final file
/// lands 0o600 too. On non-unix we fall back to `File::create` (no POSIX mode bits). This
/// is the single create point shared by `write_port_file` and `write_secret_file`; the
/// reference for the 0o600 pattern is `secrets/plain_file.rs`.
fn create_owner_only(tmp_path: &Path) -> std::io::Result<File> {
    let mut open_opts = OpenOptions::new();
    open_opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open_opts.mode(0o600);
    }
    open_opts.open(tmp_path)
}

/// Atomically write `contents` to `<dir>/<name>` via tempfile + fsync + rename, with the
/// tempfile created 0o600 (owner-only) so the rename leaves an owner-only final file.
/// Creates `dir` if it doesn't already exist. On any error the partial tempfile is
/// best-effort removed so a crashed write doesn't leave junk behind.
fn write_owner_only(dir: &Path, name: &str, contents: &[u8]) -> Result<(), PortDiscoveryError> {
    fs::create_dir_all(dir)?;
    let final_path = port_file_path(dir, name);
    let tmp_path = dir.join(format!("{name}.tmp.{}", process::id()));

    let result = (|| -> std::io::Result<()> {
        let mut file = create_owner_only(&tmp_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, &final_path)?;
        Ok(())
    })();

    if let Err(err) = result {
        let _ = fs::remove_file(&tmp_path); // best-effort cleanup
        return Err(PortDiscoveryError::Io(err));
    }
    Ok(())
}

/// Atomically write `port` to `<dir>/<name>` via tempfile + rename, 0o600 (owner-only).
/// Creates `dir` if it doesn't already exist.
pub fn write_port_file(dir: &Path, name: &str, port: u16) -> Result<(), PortDiscoveryError> {
    write_owner_only(dir, name, format!("{port}\n").as_bytes())
}

/// Atomically write a secret string (e.g. the MCP bearer token) to `<dir>/<name>` via
/// tempfile + rename, 0o600 (owner-only). No trailing newline: the token is read back
/// verbatim by external clients (`scripts/mcp-call.sh`, the E2E harness). Same atomic +
/// owner-only guarantees as `write_port_file`.
pub fn write_secret_file(dir: &Path, name: &str, secret: &str) -> Result<(), PortDiscoveryError> {
    write_owner_only(dir, name, secret.as_bytes())
}

/// Read and parse `<dir>/<name>`. Returns `NotFound` if the file isn't there yet.
///
/// Test-only: production never reads the port file from Rust (the FE uses the
/// `get_mcp_port` IPC; external tooling parses the file in shell). This exists so unit
/// tests can verify what [`write_port_file`] actually wrote.
#[cfg(test)]
pub fn read_port_file(dir: &Path, name: &str) -> Result<u16, PortDiscoveryError> {
    let path = port_file_path(dir, name);
    let raw = fs::read_to_string(&path)?;
    let trimmed = raw.trim();
    trimmed
        .parse::<u16>()
        .map_err(|_| PortDiscoveryError::InvalidContent(trimmed.to_string()))
}

/// Remove `<dir>/<name>` if it exists. Logged but not surfaced on failure: a stale file is
/// less bad than a noisy shutdown path. Callers must not depend on this for correctness;
/// external readers also retry on `ECONNREFUSED`.
pub fn remove_port_file(dir: &Path, name: &str) {
    let path = port_file_path(dir, name);
    if let Err(err) = fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        log::warn!(
            target: "mcp::port_file",
            "Could not remove port file {}: {}",
            path.display(),
            err,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_read_roundtrips() {
        let dir = tempdir().unwrap();
        write_port_file(dir.path(), "mcp.port", 12345).unwrap();
        let got = read_port_file(dir.path(), "mcp.port").unwrap();
        assert_eq!(got, 12345);
    }

    #[test]
    fn write_creates_parent_directory() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("does/not/exist/yet");
        write_port_file(&nested, "mcp.port", 19999).unwrap();
        assert_eq!(read_port_file(&nested, "mcp.port").unwrap(), 19999);
    }

    #[test]
    fn write_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        write_port_file(dir.path(), "mcp.port", 1000).unwrap();
        write_port_file(dir.path(), "mcp.port", 2000).unwrap();
        assert_eq!(read_port_file(dir.path(), "mcp.port").unwrap(), 2000);
    }

    #[test]
    fn read_missing_returns_not_found() {
        let dir = tempdir().unwrap();
        let err = read_port_file(dir.path(), "mcp.port").unwrap_err();
        assert!(matches!(err, PortDiscoveryError::NotFound));
    }

    #[test]
    fn read_garbage_returns_invalid_content() {
        let dir = tempdir().unwrap();
        let path = port_file_path(dir.path(), "mcp.port");
        fs::write(&path, "not a port\n").unwrap();
        let err = read_port_file(dir.path(), "mcp.port").unwrap_err();
        assert!(matches!(err, PortDiscoveryError::InvalidContent(ref s) if s == "not a port"));
    }

    #[test]
    fn read_trims_trailing_newline() {
        let dir = tempdir().unwrap();
        let path = port_file_path(dir.path(), "mcp.port");
        fs::write(&path, "  42\n\n").unwrap();
        assert_eq!(read_port_file(dir.path(), "mcp.port").unwrap(), 42);
    }

    #[cfg(unix)]
    #[test]
    fn written_port_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        write_port_file(dir.path(), "mcp.port", 12345).unwrap();
        let mode = fs::metadata(port_file_path(dir.path(), "mcp.port"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "port file must be 0o600 (owner-only)");
    }

    #[cfg(unix)]
    #[test]
    fn written_secret_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        write_secret_file(dir.path(), "mcp.token", "deadbeefcafef00d").unwrap();
        let mode = fs::metadata(port_file_path(dir.path(), "mcp.token"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "token file must be 0o600 (owner-only)");
    }

    #[test]
    fn secret_file_write_then_read_roundtrips() {
        let dir = tempdir().unwrap();
        write_secret_file(dir.path(), "mcp.token", "deadbeefcafef00d").unwrap();
        let got = fs::read_to_string(port_file_path(dir.path(), "mcp.token")).unwrap();
        assert_eq!(got, "deadbeefcafef00d");
    }

    #[test]
    fn remove_is_idempotent_when_file_missing() {
        let dir = tempdir().unwrap();
        remove_port_file(dir.path(), "mcp.port"); // shouldn't panic
        write_port_file(dir.path(), "mcp.port", 1234).unwrap();
        remove_port_file(dir.path(), "mcp.port");
        assert!(matches!(
            read_port_file(dir.path(), "mcp.port").unwrap_err(),
            PortDiscoveryError::NotFound
        ));
    }
}
