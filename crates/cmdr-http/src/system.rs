//! The operating system's proxy verdict for one URL.
//!
//! On macOS that's CFNetwork's own answer (`CFNetworkCopyProxiesForURL` over
//! `CFNetworkCopySystemProxySettings`), so the bypass list and "Exclude simple hostnames" match
//! exactly the way Safari and every other CFNetwork client match them, and a PAC entry is run by
//! `pac.rs`. Elsewhere there's no system layer: the environment variables are the whole
//! configuration.

use reqwest::Url;

use crate::{Route, SystemProxies};

/// The system layer [`crate::client_builder`] uses.
pub(crate) struct MacSystem;

impl SystemProxies for MacSystem {
    fn route(&self, url: &Url) -> Route {
        #[cfg(target_os = "macos")]
        {
            mac::route(url)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = url;
            Route::Direct
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) mod mac {
    use objc2_cf_network::{CFNetworkCopyProxiesForURL, CFNetworkCopySystemProxySettings};
    use objc2_core_foundation::CFDictionary;
    use reqwest::Url;

    use crate::Route;
    use crate::entries::cf::{cf_url, entries_from};
    use crate::entries::{Entry, first_route};
    use crate::pac;

    /// The current system settings' verdict for `url`, a PAC file's included.
    pub(crate) fn route(url: &Url) -> Route {
        // SAFETY: no arguments; the returned dictionary is owned (`Copy` rule) and wrapped in a
        // `CFRetained` by the binding.
        let Some(settings) = (unsafe { CFNetworkCopySystemProxySettings() }) else {
            return Route::Direct;
        };
        first_route(&entries_for(&settings, url), url, pac::shared())
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
}

#[cfg(all(test, target_os = "macos"))]
#[path = "system_test.rs"]
mod tests;
