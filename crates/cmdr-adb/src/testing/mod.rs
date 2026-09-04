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

//! Module map: `tree` (the filesystem model), `server` (the listener and the
//! wire), `shell` (the device-shell verbs).

mod server;
mod shell;
mod tree;

pub use server::FakeAdbServer;
pub use shell::{run_fake_shell, split_argv};
pub use tree::{DEFAULT_MTIME, FakeNode, FakeTree};

use crate::devices::{AdbDevice, AdbDeviceState};

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
