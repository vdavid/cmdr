//! The `sync:` service: stat, list, pull, push.
//!
//! After `sync:` → `OKAY`, the socket carries binary little-endian packets
//! `[id: 4 ASCII][arg: u32 LE]`. One session per socket; the session ends with
//! `QUIT` or by dropping the socket. Wire reference: `adb/SYNC.TXT` and
//! `file_sync_protocol.h` (verified against platform-tools 35, 2026-09).
//!
//! The v2 verbs (`STA2`, `LIS2`, `SND2`, `RCV2`) are preferred where the
//! device advertises them: they carry an errno and 64-bit sizes, which the v1
//! verbs lose (a missing path is mode 0, a 5 GB file wraps). `RCV2` and `SND2`
//! also carry the file as one LZ4 frame where the device advertises
//! `sendrecv_v2_lz4` (`lz4.rs`); callers see plain bytes either way.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::errors::AdbError;
use crate::features::{DeviceFeatures, connect_as_transport};
use crate::lz4::{Lz4Reader, Lz4Writer};
use crate::server::AdbEndpoint;
use crate::transport::AdbConnection;

/// The largest `DATA` payload the protocol allows.
pub const MAX_DATA_CHUNK: usize = 64 * 1024;

/// `kSyncFlagLZ4` in `file_sync_protocol.h`: the setup word of `RCV2` and `SND2`.
const SYNC_FLAG_LZ4: u32 = 2;

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
    /// Whether `size` is the file's own size, exactly: `STA2`/`LIS2` (a 64-bit
    /// `stat`). ❗ `false` on the v1 verbs (`STAT`/`LIST`, Android 7-era
    /// devices): their size is the low 32 bits of an `lstat`, so a file past
    /// 4 GiB wraps and a symlink reports its target path's length.
    pub size_is_exact: bool,
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

    /// The mtime as a date, whole seconds. `None` before 1970, matching the
    /// listing, which shows no date there either.
    pub fn modified_at(&self) -> Option<SystemTime> {
        let secs = u64::try_from(self.mtime).ok()?;
        UNIX_EPOCH.checked_add(Duration::from_secs(secs))
    }
}

/// The `DONE` word for a push that should land dated `date`.
///
/// ❗ `DONE` carries the mtime as a u32 on BOTH verb sets (`SND2` only adds
/// flags), so the device can't be told a date past 2106-02-07: it clamps
/// there, and a pre-1970 date clamps to the epoch. Sub-seconds are dropped.
pub fn done_mtime_word(date: SystemTime) -> u32 {
    match date.duration_since(UNIX_EPOCH) {
        Ok(since) => u32::try_from(since.as_secs()).unwrap_or(u32::MAX),
        Err(_) => 0,
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
    /// The frame of the pull in progress, when it's compressed.
    recv_frame: Option<Lz4Reader>,
    /// The frame of the push in progress, when it's compressed.
    send_frame: Option<Lz4Writer>,
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
        Ok(Self::from_connection(conn, features))
    }

    /// Wraps a connection that already speaks `sync:`.
    pub(crate) fn from_connection(conn: AdbConnection, features: DeviceFeatures) -> Self {
        Self {
            conn,
            features,
            recv_frame: None,
            send_frame: None,
        }
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

    /// Reads `[u32 len][bytes]`, refusing a length past [`MAX_DATA_CHUNK`]
    /// before allocating: the device chooses the length word, and nothing on
    /// this wire is longer than a `DATA` chunk (the reference client caps names
    /// at 255 bytes and messages at `SYNC_DATA_MAX`; `client/file_sync_client.cpp`,
    /// AOSP `packages/modules/adb` main, 2026-10-05).
    async fn read_len_prefixed(&mut self) -> Result<Vec<u8>, AdbError> {
        let len = self.conn.read_u32_le().await? as usize;
        if len > MAX_DATA_CHUNK {
            return Err(AdbError::Protocol(format!(
                // allowed-pluralize-noun: `len` is past the cap here, so never 1.
                "sync payload of {len} bytes exceeds the {MAX_DATA_CHUNK}-byte maximum"
            )));
        }
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
            size_is_exact: false,
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
            size_is_exact: true,
            mtime,
            errno: (error != 0).then(|| i32::try_from(error).unwrap_or(i32::MAX)),
        })
    }

    /// Lists `path`, handing each entry to `on_entry` as it arrives. `LIS2`
    /// where the device has it, else `LIST`. `.`, `..`, and any name that isn't
    /// one path segment are skipped.
    ///
    /// ❗ A missing, an unreadable, and an empty directory all list as EMPTY on
    /// both verbs (the device answers `DONE` straight away). A stat separates
    /// "missing", but it succeeds on a folder the device won't open, so telling
    /// "unreadable" from "empty" takes the shell's `test -r` / `test -x` (the
    /// volume's `refuse_if_unreadable`). A real `adbd` sends no `.`/`..`; the
    /// skip is for a device that does, or that names an entry `a/b`.
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
        let compress = self.features.lz4_transfers();
        self.recv_frame = compress.then(Lz4Reader::new);
        if self.features.sendrecv_v2 {
            self.send_payload(b"RCV2", path.as_bytes()).await?;
            // The setup word: the flags. No dry run.
            self.send_word(b"RCV2", if compress { SYNC_FLAG_LZ4 } else { 0 }).await
        } else {
            self.send_payload(b"RECV", path.as_bytes()).await
        }
    }

    /// The next chunk of the file being pulled, at most [`MAX_DATA_CHUNK`] bytes
    /// of the file itself: `Some(bytes)` on `DATA`, `None` on `DONE`,
    /// [`AdbError::Refused`] on `FAIL`.
    ///
    /// A compressed pull decodes one chunk per call, so what a small packet
    /// expands to waits behind the caller's own pace.
    pub async fn recv_chunk(&mut self) -> Result<Option<Vec<u8>>, AdbError> {
        loop {
            if let Some(frame) = &mut self.recv_frame
                && let Some(chunk) = frame.next_chunk()?
            {
                return Ok(Some(chunk));
            }
            match &self.conn.read_id().await? {
                b"DATA" => {
                    let payload = self.read_len_prefixed().await?;
                    match &mut self.recv_frame {
                        Some(frame) => frame.feed(&payload),
                        None => return Ok(Some(payload)),
                    }
                }
                b"DONE" => {
                    let _zero = self.conn.read_u32_le().await?;
                    if let Some(frame) = self.recv_frame.take() {
                        frame.finish()?;
                    }
                    return Ok(None);
                }
                b"FAIL" => return Err(self.read_fail().await),
                other => return Err(unexpected("DATA", other)),
            }
        }
    }

    /// Starts pushing to `path` with `mode`. Follow with
    /// [`SyncSession::send_chunk`] then [`SyncSession::send_finish`].
    pub async fn send_start(&mut self, path: &str, mode: u32) -> Result<(), AdbError> {
        let compress = self.features.lz4_transfers();
        self.send_frame = compress.then(Lz4Writer::new);
        if self.features.sendrecv_v2 {
            self.send_payload(b"SND2", path.as_bytes()).await?;
            // The setup packet: id, mode, flags. No dry run.
            let flags: u32 = if compress { SYNC_FLAG_LZ4 } else { 0 };
            let mut packet = Vec::with_capacity(12);
            packet.extend_from_slice(b"SND2");
            packet.extend_from_slice(&mode.to_le_bytes());
            packet.extend_from_slice(&flags.to_le_bytes());
            self.conn.write_all(&packet).await
        } else {
            let spec = format!("{path},{mode}");
            self.send_payload(b"SEND", spec.as_bytes()).await
        }
    }

    /// Pushes `bytes`, split into `DATA` packets of at most
    /// [`MAX_DATA_CHUNK`]. A compressed push holds back what doesn't fill a
    /// packet yet, so the device sees it by [`SyncSession::send_finish`] at the
    /// latest.
    pub async fn send_chunk(&mut self, bytes: &[u8]) -> Result<(), AdbError> {
        for chunk in bytes.chunks(MAX_DATA_CHUNK) {
            match &mut self.send_frame {
                Some(frame) => {
                    let ready = frame.write(chunk)?;
                    self.send_packets(&ready).await?;
                }
                None => self.send_payload(b"DATA", chunk).await?,
            }
        }
        Ok(())
    }

    /// Writes `bytes` as `DATA` packets of at most [`MAX_DATA_CHUNK`].
    async fn send_packets(&mut self, bytes: &[u8]) -> Result<(), AdbError> {
        for packet in bytes.chunks(MAX_DATA_CHUNK) {
            self.send_payload(b"DATA", packet).await?;
        }
        Ok(())
    }

    /// Ends the push with `DONE` + `mtime` and reads the device's verdict.
    pub async fn send_finish(&mut self, mtime: u32) -> Result<(), AdbError> {
        if let Some(frame) = self.send_frame.take() {
            let rest = frame.finish()?;
            self.send_packets(&rest).await?;
        }
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

/// Hands one listed entry on, unless its name isn't a single path segment: a
/// device naming one `..`, empty, or with a `/` in it would have the volume join
/// it into a path naming some other file, so it's dropped, ❌ never listed.
fn push_entry(on_entry: &mut (dyn FnMut(SyncDirEntry) + Send), name: Vec<u8>, stat: SyncStat) {
    let name = String::from_utf8_lossy(&name).into_owned();
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
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
