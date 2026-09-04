//! The ADB host-protocol socket: the ONE module that knows the framing.
//!
//! A request is four ASCII hex digits of length, then the payload
//! (`000Chost:version`). The answer is `OKAY` or `FAIL`; `FAIL` is followed by
//! a 4-hex-length message. Payload-bearing answers (`host:devices-l`,
//! `host:track-devices` pushes) are one 4-hex-length message each. After
//! `host:transport:<serial>`, the socket is bound to that device and the next
//! request names a device service (`sync:`, `shell,v2,raw:<cmd>`), whose own
//! binary framing `sync.rs` and `shell.rs` read through the raw helpers here.
//!
//! Wire reference: `adb/protocol.txt` in the AOSP platform tools (verified
//! against platform-tools 35, 2026-09).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::errors::AdbError;

/// One TCP socket to the ADB server.
///
/// Every method takes `&mut self`: the protocol is strictly request/response
/// (or, after `sync:` / `shell`, a single stream), so two callers can't share
/// one socket.
pub struct AdbConnection {
    stream: TcpStream,
}

impl std::fmt::Debug for AdbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdbConnection").finish_non_exhaustive()
    }
}

impl AdbConnection {
    /// Wraps a connected socket. The caller has already dialed; see
    /// [`crate::server::AdbEndpoint::connect`].
    pub(crate) fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }

    /// Sends one host request and reads its status. `OKAY` is `Ok(())`; `FAIL`
    /// is [`AdbError::Refused`] carrying the server's message.
    pub async fn request(&mut self, service: &str) -> Result<(), AdbError> {
        self.write_all(&frame(service)).await?;
        self.read_status().await
    }

    /// Reads a 4-hex-length payload: what `host:devices-l` answers after `OKAY`,
    /// and what `host:track-devices` pushes for the life of the socket.
    pub async fn read_hex_message(&mut self) -> Result<Vec<u8>, AdbError> {
        let mut len = [0u8; 4];
        self.read_exact(&mut len).await?;
        let len = parse_hex_len(&len)?;
        let mut payload = vec![0u8; len];
        self.read_exact(&mut payload).await?;
        Ok(payload)
    }

    /// Binds this socket to the device with `serial` (`host:transport:<serial>`).
    /// The next request is a device service.
    pub async fn bind_device(&mut self, serial: &str) -> Result<(), AdbError> {
        self.request(&format!("host:transport:{serial}")).await
    }

    /// Fills `buf` from the socket. A peer that hangs up first is
    /// [`AdbError::DeviceGone`].
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), AdbError> {
        self.stream.read_exact(buf).await?;
        Ok(())
    }

    /// Writes all of `buf` to the socket.
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), AdbError> {
        self.stream.write_all(buf).await?;
        Ok(())
    }

    /// Reads one little-endian `u32`, the sync service's argument word.
    pub async fn read_u32_le(&mut self) -> Result<u32, AdbError> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf).await?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Reads one little-endian `u64`, the `STA2`/`DNT2` wide fields.
    pub async fn read_u64_le(&mut self) -> Result<u64, AdbError> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf).await?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Reads one little-endian `i64`, the `STA2`/`DNT2` timestamps.
    pub async fn read_i64_le(&mut self) -> Result<i64, AdbError> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf).await?;
        Ok(i64::from_le_bytes(buf))
    }

    /// Reads a four-byte ASCII id (`OKAY`, `FAIL`, `DATA`, `DENT`, …).
    pub async fn read_id(&mut self) -> Result<[u8; 4], AdbError> {
        let mut id = [0u8; 4];
        self.read_exact(&mut id).await?;
        Ok(id)
    }

    /// Reads an `OKAY`/`FAIL` status word, turning a `FAIL` into
    /// [`AdbError::Refused`] with its 4-hex-length message.
    pub async fn read_status(&mut self) -> Result<(), AdbError> {
        match &self.read_id().await? {
            b"OKAY" => Ok(()),
            b"FAIL" => {
                let msg = self.read_hex_message().await?;
                Err(AdbError::Refused(String::from_utf8_lossy(&msg).into_owned()))
            }
            other => Err(AdbError::Protocol(format!(
                "expected OKAY or FAIL, got {:?}",
                String::from_utf8_lossy(other)
            ))),
        }
    }

    /// Closes the write side. Best effort: a socket that's already gone has
    /// nothing left to say.
    pub async fn shutdown(&mut self) {
        let _ = self.stream.shutdown().await;
    }
}

/// Frames one host request: four lowercase hex digits of length, then the
/// service string. Pure, so the framing is testable without a socket.
pub(crate) fn frame(service: &str) -> Vec<u8> {
    let mut out = format!("{:04x}", service.len()).into_bytes();
    out.extend_from_slice(service.as_bytes());
    out
}

/// Frames one 4-hex-length payload the way the SERVER writes it (an
/// `OKAY`-following message or a `FAIL` reason). Used by the fake server.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn hex_message(payload: &[u8]) -> Vec<u8> {
    let mut out = format!("{:04x}", payload.len()).into_bytes();
    out.extend_from_slice(payload);
    out
}

/// Decodes a four-digit hex length. Either case is accepted, since the server
/// writes lowercase and the reference client uppercase.
pub(crate) fn parse_hex_len(digits: &[u8; 4]) -> Result<usize, AdbError> {
    let text = std::str::from_utf8(digits).map_err(|_| AdbError::Protocol("length is not ASCII".to_string()))?;
    usize::from_str_radix(text, 16).map_err(|_| AdbError::Protocol(format!("length is not hex: {text:?}")))
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod transport_test;
