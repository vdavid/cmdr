//! macOS: CFNetwork's own answer (`CFNetworkCopyProxiesForURL` over
//! `CFNetworkCopySystemProxySettings`), so the bypass list and "Exclude simple hostnames" match
//! exactly the way Safari and every other CFNetwork client match them, and a PAC entry is run by
//! `pac.rs`.

use std::sync::Arc;

use objc2_cf_network::{CFNetworkCopyProxiesForURL, CFNetworkCopySystemProxySettings};
use objc2_core_foundation::CFDictionary;
use reqwest::Url;

use crate::Route;
use crate::entries::cf::{cf_url, entries_from};
use crate::entries::{Entry, Pac, first_route};
use crate::pac;

/// The current system settings' verdict for `url`, a PAC file's included.
pub(crate) fn route(url: &Url) -> Route {
    route_with(url, pac::shared())
}

/// [`route`], with a PAC entry run by `pac`.
pub(crate) fn route_with(url: &Url, pac: &dyn Pac) -> Route {
    // SAFETY: no arguments; the returned dictionary is owned (`Copy` rule) and wrapped in a
    // `CFRetained` by the binding.
    let Some(settings) = (unsafe { CFNetworkCopySystemProxySettings() }) else {
        return Route::Direct;
    };
    first_route(&entries_for(&settings, url), url, pac)
}

/// CFNetwork's ordered answer for `url` under `settings`. Split out so a test can pass its own
/// settings dictionary instead of the Mac's.
pub(crate) fn entries_for(settings: &CFDictionary, url: &Url) -> Vec<Entry> {
    let Some(target) = cf_url(url.as_str()) else {
        return Vec::new();
    };
    // SAFETY: `settings` is a proxy-settings dictionary (string keys, CF values), which is the
    // only "generics" requirement the binding names; `target` is a valid CFURL.
    let list = unsafe { CFNetworkCopyProxiesForURL(&target, settings) };
    entries_from(&list)
}

/// A proxy-settings dictionary for a test, in place of the Mac's own.
#[cfg(test)]
pub(crate) fn settings(
    pairs: &[(&objc2_core_foundation::CFString, &objc2_core_foundation::CFType)],
) -> objc2_core_foundation::CFRetained<CFDictionary> {
    use objc2_core_foundation::{CFRetained, CFString, CFType};
    let keys: Vec<&CFString> = pairs.iter().map(|(key, _)| *key).collect();
    let values: Vec<&CFType> = pairs.iter().map(|(_, value)| *value).collect();
    let typed = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: a typed dictionary is the same CF object as its untyped view.
    unsafe { CFRetained::cast_unchecked(typed) }
}

/// The system layer [`warm_up`] decides with: this arm's [`route`], but a PAC miss gets
/// [`pac::WARM_UP_TIMEOUT`], since nothing waits on it but the
/// requests that would otherwise have run the cold first evaluation themselves.
struct WarmingSystem;

impl crate::SystemProxies for WarmingSystem {
    fn route(&self, url: &Url) -> Route {
        route_with(url, &pac::shared().with_budget(pac::WARM_UP_TIMEOUT))
    }
}

/// Runs the PAC evaluation for `url` once, on a background thread.
pub(crate) fn warm_up(url: Url, env: Arc<crate::env::EnvProxies>) {
    let spawned = std::thread::Builder::new()
        .name(String::from("cmdr-http-pac-warm-up"))
        .spawn(move || {
            // The answer lands in the PAC cache; the route itself is nobody's to act on.
            let _ = crate::route::decide(&url, &env, &WarmingSystem);
        });
    if spawned.is_err() {
        log::debug!(target: "cmdr_http", "couldn't start the PAC warm-up; the first request runs it instead");
    }
}

#[cfg(test)]
#[path = "macos_test.rs"]
mod tests;
