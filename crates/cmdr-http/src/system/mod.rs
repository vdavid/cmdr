//! The operating system's proxy verdict for one URL, behind a per-OS seam (`platform/`). On
//! macOS that's CFNetwork's answer; on Linux and Windows there's no system layer yet, and the
//! environment variables are the whole configuration.

mod platform;

use std::sync::Arc;

use reqwest::Url;

use crate::env::EnvProxies;
use crate::{Route, SystemProxies};

/// The system layer [`crate::client_builder`] uses: this OS's own answer for a URL.
pub(crate) struct OsSystem;

impl SystemProxies for OsSystem {
    fn route(&self, url: &Url) -> Route {
        platform::route(url)
    }
}

/// Runs this OS's slow first proxy evaluation for `url` ahead of the requests that need it
/// (a PAC file, on macOS), or nothing where the OS has none.
pub(crate) fn warm_up(url: Url, env: Arc<EnvProxies>) {
    platform::warm_up(url, env);
}
