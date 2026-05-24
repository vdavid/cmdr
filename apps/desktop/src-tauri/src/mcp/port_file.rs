//! Atomic port-file IO for the Cmdr MCP server (and a sibling file for the tauri-MCP
//! bridge plugin). External readers (the `scripts/mcp-call.sh` CLI, E2E fixtures, agent
//! helpers) discover the actual bound port by reading `<data_dir>/<name>.port` rather than
//! guessing the configured default. The in-process FE keeps using the `get_mcp_port` IPC
//! (it reads the same `MCP_ACTUAL_PORT` atomic). See `docs/specs/instance-isolation-plan.md`
//! § P2 for the full contract.
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

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

/// Errors that can surface from the port file. Typed so call sites can branch without
/// substring matching the message (AGENTS.md "no string-matching error classification").
///
/// Today only the writer side ships in this binary (the FE reads `MCP_ACTUAL_PORT` via
/// IPC). The reader / typed errors are pub so the in-tree port-file consumers we'll wire
/// in later phases (P3 checker shard rewire; the Bash CLI lives in `scripts/`) share one
/// vocabulary. Allowed-unused with a reason because the crate root denies `unused`.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "reader API used by tests today; phase-3 checker will pick it up"
)]
pub enum PortDiscoveryError {
    /// The port file doesn't exist yet (or anymore).
    NotFound,
    /// Filesystem error reading or writing the file.
    Io(std::io::Error),
    /// The file exists but its contents don't parse as `u16` + optional newline.
    InvalidContent(String),
}

impl std::fmt::Display for PortDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "port file not found"),
            Self::Io(err) => write!(f, "port file IO error: {err}"),
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

/// Atomically write `port` to `<dir>/<name>` via tempfile + rename. Creates `dir` if it
/// doesn't already exist. On any error the partial tempfile is best-effort removed so a
/// crashed write doesn't leave junk behind.
pub fn write_port_file(dir: &Path, name: &str, port: u16) -> Result<(), PortDiscoveryError> {
    fs::create_dir_all(dir)?;
    let final_path = port_file_path(dir, name);
    let tmp_path = dir.join(format!("{name}.tmp.{}", process::id()));

    let result = (|| -> std::io::Result<()> {
        let mut file = File::create(&tmp_path)?;
        writeln!(file, "{port}")?;
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

/// Read and parse `<dir>/<name>`. Returns `NotFound` if the file isn't there yet.
#[allow(
    dead_code,
    reason = "in-tree reader: tests use it today; phase-3 checker will pick it up"
)]
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
