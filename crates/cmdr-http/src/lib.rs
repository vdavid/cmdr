#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The one door every Cmdr HTTP client is built through, and the proxy routing behind it.
//!
//! [`client_builder`] is `reqwest::Client::builder()` plus Cmdr's proxy decision, made per
//! destination: loopback and link-local hosts always go direct, then the `*_PROXY` environment
//! variables decide, then macOS's own answer for that URL (the proxy settings, their bypass list,
//! and a PAC file). `DETAILS.md` has the order and the why.

mod entries;
mod env;
#[cfg(target_os = "macos")]
mod pac;
mod route;
mod system;

use std::sync::{Arc, OnceLock};

use env::EnvProxies;
use reqwest::Url;

/// The routing verdict for one destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    Direct,
    /// A proxy URL reqwest can tunnel or forward through: `http://host:port` or
    /// `socks5h://host:port` (or an environment variable's own `socks4` / `socks5` / ... URL), with
    /// `user:password@` when the source carried credentials.
    Proxy(String),
}

/// What the operating system says about a URL, once the earlier layers have had no say.
pub(crate) trait SystemProxies: Send + Sync {
    fn route(&self, url: &Url) -> Route;
}

/// A `reqwest::ClientBuilder` that routes every request the way macOS would, minus the cases
/// where that hurts (a proxy can't reach this Mac's own loopback). Callers add their own timeouts,
/// redirect policy, and decoders on top. ❌ Never call `reqwest::Client::builder()` directly:
/// the workspace `clippy.toml` refuses it outside this crate.
pub fn client_builder() -> reqwest::ClientBuilder {
    static ENV: OnceLock<Arc<EnvProxies>> = OnceLock::new();
    let env = ENV.get_or_init(|| Arc::new(EnvProxies::from_process())).clone();
    builder_with(env, Arc::new(system::MacSystem))
}

/// [`client_builder`] with its two inputs injected, so a test can route without touching the
/// process environment or the Mac's settings.
fn builder_with(env: Arc<EnvProxies>, system: Arc<dyn SystemProxies>) -> reqwest::ClientBuilder {
    #[allow(
        clippy::disallowed_methods,
        reason = "this is the one door the rule points everyone else to"
    )]
    let builder = reqwest::Client::builder();
    // A `Proxy::custom` turns reqwest's own system-proxy reading off (`ClientBuilder::proxy`), so
    // this closure is the whole decision. reqwest calls it with `scheme://host[:port]`, per
    // connection and, for plain-HTTP requests, per request.
    builder.proxy(reqwest::Proxy::custom(move |url| {
        match route::decide(url, &env, system.as_ref()) {
            Route::Direct => None,
            Route::Proxy(proxy) => Some(proxy),
        }
    }))
}

#[cfg(test)]
mod lib_test;
