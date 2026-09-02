//! The `sync:` service: stat, list, pull, push.
//!
//! After `sync:` → `OKAY`, the socket carries binary little-endian packets
//! `[id: 4 ASCII][arg: u32 LE]`. One session per socket; the session ends with
//! `QUIT` or by dropping the socket. Wire reference: `adb/SYNC.TXT` and
//! `file_sync_protocol.h` (verified against platform-tools 35, 2026-09).
//!
//! The v2 verbs (`STA2`, `LIS2`, `SND2`, `RCV2`) are preferred where the
//! device advertises them: they carry an errno and 64-bit sizes, which the v1
//! verbs lose (a missing path is mode 0, a 5 GB file wraps).

use crate::errors::AdbError;
use crate::features::{DeviceFeatures, connect_as_transport};
use crate::server::AdbEndpoint;
use crate::transport::AdbConnection;

/// The largest `DATA` payload the protocol allows.
pub const MAX_DATA_CHUNK: usize = 64 * 1024;

/// The `S_IFMT` mask and the file-type bits the mode word carries.
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;

/// What kind of node a mode word describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncEntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symlink (the sync service never follows them).
    Symlink,
    /// A socket, device, fifo, or a mode of 0.
    Other,
}

/// A `STAT`/`STA2` answer, or the stat half of a directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncStat {
    /// The POSIX mode word (type bits and permissions).
    pub mode: u32,
    /// Size in bytes. 32-bit on the v1 verbs.
    pub size: u64,
    /// Modification time, seconds since the epoch.
    pub mtime: i64,
    /// The device's errno when the stat itself failed (v2 only). `None` on v1,
    /// where a mode of 0 is the only "not there" signal.
    pub errno: Option<i32>,
}

impl SyncStat {
    /// The node type the mode word names.
    pub fn kind(&self) -> SyncEntryKind {
        match self.mode & S_IFMT {
            S_IFDIR => SyncEntryKind::Directory,
            S_IFREG => SyncEntryKind::File,
            S_IFLNK => SyncEntryKind::Symlink,
            _ => SyncEntryKind::Other,
        }
    }

    /// Whether the stat found something: no errno and a non-zero mode.
    pub fn exists(&self) -> bool {
        self.errno.is_none() && self.mode != 0
    }
}

/// One entry of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDirEntry {
    /// The entry's name, never `.` or `..`.
    pub name: String,
    /// Its stat, as the listing verb reports it.
    pub stat: SyncStat,
}

/// One open `sync:` session.
pub struct SyncSession {
    conn: AdbConnection,
    features: DeviceFeatures,
}

impl std::fmt::Debug for SyncSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncSession")
            .field("features", &self.features)
            .finish_non_exhaustive()
    }
}

impl SyncSession {
    /// Connects, binds to `serial`, and opens the sync service.
    pub async fn open(endpoint: &AdbEndpoint, serial: &str, features: DeviceFeatures) -> Result<Self, AdbError> {
        let mut conn = endpoint.connect().await.map_err(connect_as_transport)?;
        conn.bind_device(serial).await?;
        conn.request("sync:").await?;
        Ok(Self { conn, features })
    }

    /// Writes `[id][u32 LE]`.
    async fn send_word(&mut self, id: &[u8; 4], arg: u32) -> Result<(), AdbError> {
        let mut packet = Vec::with_capacity(8);
        packet.extend_from_slice(id);
        packet.extend_from_slice(&arg.to_le_bytes());
        self.conn.write_all(&packet).await
    }

    /// Writes `[id][len][payload]`.
    async fn send_payload(&mut self, id: &[u8; 4], payload: &[u8]) -> Result<(), AdbError> {
        let len =
            u32::try_from(payload.len()).map_err(|_| AdbError::Protocol("payload longer than u32".to_string()))?;
        let mut packet = Vec::with_capacity(8 + payload.len());
        packet.extend_from_slice(id);
        packet.extend_from_slice(&len.to_le_bytes());
        packet.extend_from_slice(payload);
        self.conn.write_all(&packet).await
    }

    /// Reads a `FAIL` payload into [`AdbError::Refused`].
    async fn read_fail(&mut self) -> AdbError {
        match self.read_len_prefixed().await {
            Ok(msg) => AdbError::Refused(String::from_utf8_lossy(&msg).into_owned()),
            Err(err) => err,
        }
    }

    /// Reads `[u32 len][bytes]`.
    async fn read_len_prefixed(&mut self) -> Result<Vec<u8>, AdbError> {
        let len = self.conn.read_u32_le().await? as usize;
        let mut buf = vec![0u8; len];
        self.conn.read_exact(&mut buf).await?;
        Ok(buf)
    }

    /// Stats `path`. `STA2` where the device has it (errno on failure), else
    /// `STAT` (mode 0 on failure). A missing path is `Ok` with
    /// [`SyncStat::exists`] false, never `Err`.
    pub async fn stat(&mut self, path: &str) -> Result<SyncStat, AdbError> {
        if self.features.stat_v2 {
            self.send_payload(b"STA2", path.as_bytes()).await?;
            match &self.conn.read_id().await? {
                b"STA2" => self.read_stat_v2().await,
                b"FAIL" => Err(self.read_fail().await),
                other => Err(unexpected("STA2", other)),
            }
        } else {
            self.send_payload(b"STAT", path.as_bytes()).await?;
            match &self.conn.read_id().await? {
                b"STAT" => self.read_stat_v1().await,
                b"FAIL" => Err(self.read_fail().await),
                other => Err(unexpected("STAT", other)),
            }
        }
    }

    /// Reads the body of a `STAT` answer: mode, size, mtime (u32 each).
    async fn read_stat_v1(&mut self) -> Result<SyncStat, AdbError> {
        let mode = self.conn.read_u32_le().await?;
        let size = self.conn.read_u32_le().await?;
        let mtime = self.conn.read_u32_le().await?;
        Ok(SyncStat {
            mode,
            size: u64::from(size),
            mtime: i64::from(mtime),
            errno: None,
        })
    }

    /// Reads the body of a `STA2`/`DNT2` answer (everything after the id, up
    /// to and excluding `DNT2`'s namelen).
    async fn read_stat_v2(&mut self) -> Result<SyncStat, AdbError> {
        let error = self.conn.read_u32_le().await?;
        let _dev = self.conn.read_u64_le().await?;
        let _ino = self.conn.read_u64_le().await?;
        let mode = self.conn.read_u32_le().await?;
        let _nlink = self.conn.read_u32_le().await?;
        let _uid = self.conn.read_u32_le().await?;
        let _gid = self.conn.read_u32_le().await?;
        let size = self.conn.read_u64_le().await?;
        let _atime = self.conn.read_i64_le().await?;
        let mtime = self.conn.read_i64_le().await?;
        let _ctime = self.conn.read_i64_le().await?;
        Ok(SyncStat {
            mode,
            size,
            mtime,
            errno: (error != 0).then(|| i32::try_from(error).unwrap_or(i32::MAX)),
        })
    }

    /// Lists `path`, handing each entry to `on_entry` as it arrives. `LIS2`
    /// where the device has it, else `LIST`. `.` and `..` are skipped.
    ///
    /// ❗ A missing or unreadable directory lists as EMPTY on both verbs (the
    /// device answers `DONE` straight away); callers that need to tell those
    /// apart stat the path first.
    pub async fn list(&mut self, path: &str, on_entry: &mut (dyn FnMut(SyncDirEntry) + Send)) -> Result<(), AdbError> {
        if self.features.ls_v2 {
            self.send_payload(b"LIS2", path.as_bytes()).await?;
            loop {
                match &self.conn.read_id().await? {
                    b"DNT2" => {
                        let stat = self.read_stat_v2().await?;
                        let name = self.read_len_prefixed().await?;
                        push_entry(on_entry, name, stat);
                    }
                    b"DONE" => {
                        // `DONE` is a zeroed dent_v2 minus the id: 72 bytes.
                        let mut rest = [0u8; 72];
                        self.conn.read_exact(&mut rest).await?;
                        return Ok(());
                    }
                    b"FAIL" => return Err(self.read_fail().await),
                    other => return Err(unexpected("DNT2", other)),
                }
            }
        } else {
            self.send_payload(b"LIST", path.as_bytes()).await?;
            loop {
                match &self.conn.read_id().await? {
                    b"DENT" => {
                        let stat = self.read_stat_v1().await?;
                        let name = self.read_len_prefixed().await?;
                        push_entry(on_entry, name, stat);
                    }
                    b"DONE" => {
                        // `DONE` is a zeroed dent_v1 minus the id: 16 bytes.
                        let mut rest = [0u8; 16];
                        self.conn.read_exact(&mut rest).await?;
                        return Ok(());
                    }
                    b"FAIL" => return Err(self.read_fail().await),
                    other => return Err(unexpected("DENT", other)),
                }
            }
        }
    }

    /// Starts pulling `path`. Follow with [`SyncSession::recv_chunk`] until it
    /// answers `None`.
    pub async fn recv_start(&mut self, path: &str) -> Result<(), AdbError> {
        if self.features.sendrecv_v2 {
            self.send_payload(b"RCV2", path.as_bytes()).await?;
            // The setup word: flags 0 (no compression, no dry run).
            self.send_word(b"RCV2", 0).await
        } else {
            self.send_payload(b"RECV", path.as_bytes()).await
        }
    }

    /// The next chunk of the file being pulled: `Some(bytes)` on `DATA`, `None`
    /// on `DONE`, [`AdbError::Refused`] on `FAIL`.
    pub async fn recv_chunk(&mut self) -> Result<Option<Vec<u8>>, AdbError> {
        match &self.conn.read_id().await? {
            b"DATA" => Ok(Some(self.read_len_prefixed().await?)),
            b"DONE" => {
                let _zero = self.conn.read_u32_le().await?;
                Ok(None)
            }
            b"FAIL" => Err(self.read_fail().await),
            other => Err(unexpected("DATA", other)),
        }
    }

    /// Starts pushing to `path` with `mode`. Follow with
    /// [`SyncSession::send_chunk`] then [`SyncSession::send_finish`].
    pub async fn send_start(&mut self, path: &str, mode: u32) -> Result<(), AdbError> {
        if self.features.sendrecv_v2 {
            self.send_payload(b"SND2", path.as_bytes()).await?;
            // The setup packet: id, mode, flags 0.
            let mut packet = Vec::with_capacity(12);
            packet.extend_from_slice(b"SND2");
            packet.extend_from_slice(&mode.to_le_bytes());
            packet.extend_from_slice(&0u32.to_le_bytes());
            self.conn.write_all(&packet).await
        } else {
            let spec = format!("{path},{mode}");
            self.send_payload(b"SEND", spec.as_bytes()).await
        }
    }

    /// Pushes `bytes`, split into `DATA` packets of at most
    /// [`MAX_DATA_CHUNK`].
    pub async fn send_chunk(&mut self, bytes: &[u8]) -> Result<(), AdbError> {
        for chunk in bytes.chunks(MAX_DATA_CHUNK) {
            self.send_payload(b"DATA", chunk).await?;
        }
        Ok(())
    }

    /// Ends the push with `DONE` + `mtime` and reads the device's verdict.
    pub async fn send_finish(&mut self, mtime: u32) -> Result<(), AdbError> {
        self.send_word(b"DONE", mtime).await?;
        match &self.conn.read_id().await? {
            b"OKAY" => {
                let _zero = self.conn.read_u32_le().await?;
                Ok(())
            }
            b"FAIL" => Err(self.read_fail().await),
            other => Err(unexpected("OKAY", other)),
        }
    }

    /// Ends the session. Best effort: the device tears the socket down either
    /// way.
    pub async fn quit(mut self) {
        let _ = self.send_word(b"QUIT", 0).await;
        self.conn.shutdown().await;
    }
}

fn push_entry(on_entry: &mut (dyn FnMut(SyncDirEntry) + Send), name: Vec<u8>, stat: SyncStat) {
    let name = String::from_utf8_lossy(&name).into_owned();
    if name == "." || name == ".." {
        return;
    }
    on_entry(SyncDirEntry { name, stat });
}

fn unexpected(wanted: &str, got: &[u8; 4]) -> AdbError {
    AdbError::Protocol(format!("expected {wanted}, got {:?}", String::from_utf8_lossy(got)))
}

#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;
