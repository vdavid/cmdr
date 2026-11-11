//! Picks this OS's proxy reader.

use std::sync::Arc;

use reqwest::Url;

use crate::Route;
use crate::env::EnvProxies;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&Url) -> Route = route;
const _: fn(Url, Arc<EnvProxies>) = warm_up;

/// The current system settings' verdict for `url`, a PAC file's included.
pub(super) use imp::route;

/// Runs the OS's slow first evaluation for `url` on a background thread, so a request doesn't
/// wait on it; nothing where the OS has no such step.
pub(super) use imp::warm_up;
