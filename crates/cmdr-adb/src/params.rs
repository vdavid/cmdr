//! How to reach one device, and how to reach it again.

use crate::server::AdbEndpoint;

/// Everything needed to open (and later re-open) one ADB volume.
///
/// No secret lives here: authorization is the phone's RSA prompt, which the
/// ADB server owns.
#[derive(Debug, Clone)]
pub struct AdbConnectionParams {
    /// The device serial as `host:devices` lists it. ❗ The volume's whole
    /// identity: `volume_id` is `adb:<serial>`.
    pub serial: String,
    /// The ADB server to ask for that device.
    pub endpoint: AdbEndpoint,
}

impl AdbConnectionParams {
    /// Params for the common case: the local server on `127.0.0.1:5037`.
    pub fn new(serial: &str) -> Self {
        Self::at(serial, AdbEndpoint::default_local())
    }

    /// Params for a device behind a specific server (tests, a fake, a
    /// forwarded port).
    pub fn at(serial: &str, endpoint: AdbEndpoint) -> Self {
        Self {
            serial: serial.to_string(),
            endpoint,
        }
    }
}
