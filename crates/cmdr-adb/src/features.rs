//! What a device's own ADB supports, read once per session.
//!
//! `host-serial:<serial>:features` answers the comma-separated list `adbd`
//! advertised at connect (`shell_v2,cmd,stat_v2,ls_v2,fixed_push_mkdir,…`).
//! The four that change what this crate sends are recorded; the rest are
//! ignored.

use crate::errors::AdbError;
use crate::server::AdbEndpoint;

/// The feature flags this crate branches on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceFeatures {
    /// `shell,v2,raw:` with framed stdout/stderr/exit. ❗ Without it the device
    /// is refused (`AdbConnectError::DeviceTooOld`): the legacy `shell:` has no
    /// exit code.
    pub shell_v2: bool,
    /// `STA2`: stat with errno and 64-bit sizes.
    pub stat_v2: bool,
    /// `LIS2`: listing with errno and 64-bit sizes.
    pub ls_v2: bool,
    /// `SND2`/`RCV2`: transfers with a flags word (compression, dry-run).
    pub sendrecv_v2: bool,
}

impl DeviceFeatures {
    /// Parses the server's comma-separated feature list. Unknown names are
    /// ignored; whitespace around names is tolerated.
    pub fn parse(list: &str) -> Self {
        let mut features = Self::default();
        for name in list.split(',').map(str::trim) {
            match name {
                "shell_v2" => features.shell_v2 = true,
                "stat_v2" => features.stat_v2 = true,
                "ls_v2" => features.ls_v2 = true,
                "sendrecv_v2" => features.sendrecv_v2 = true,
                _ => {}
            }
        }
        features
    }

    /// Everything on: what a current device advertises. For fixtures.
    pub fn all() -> Self {
        Self {
            shell_v2: true,
            stat_v2: true,
            ls_v2: true,
            sendrecv_v2: true,
        }
    }

    /// Asks the server what the device with `serial` supports.
    pub async fn fetch(endpoint: &AdbEndpoint, serial: &str) -> Result<Self, AdbError> {
        let mut conn = endpoint.connect().await.map_err(connect_as_transport)?;
        conn.request(&format!("host-serial:{serial}:features")).await?;
        let list = conn.read_hex_message().await?;
        conn.shutdown().await;
        Ok(Self::parse(&String::from_utf8_lossy(&list)))
    }
}

/// A connect failure seen from a call that promised an [`AdbError`].
///
/// The typed connect variants that matter (`AdbNotInstalled`, `Unauthorized`)
/// arise before any device call, at the volume's own connect; by the time a
/// feature fetch or a shell run dials, a refused socket means the server left.
pub(crate) fn connect_as_transport(err: crate::errors::AdbConnectError) -> AdbError {
    use crate::errors::AdbConnectError;
    match err {
        AdbConnectError::TimedOut => AdbError::Timeout,
        AdbConnectError::Cancelled => AdbError::Cancelled,
        AdbConnectError::DeviceGone(_) => AdbError::DeviceGone,
        other => AdbError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            other.to_string(),
        )),
    }
}

#[cfg(test)]
#[path = "features_test.rs"]
mod features_test;
