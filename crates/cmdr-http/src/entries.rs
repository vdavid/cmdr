//! CFNetwork's ordered proxy list, as Cmdr reads and walks it. The list comes from the system
//! settings (`system.rs`) or from a PAC file (`pac.rs`), in the same shape either way.
#![cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the list walk serves macOS's answer; elsewhere only its tests use it"
    )
)]

use reqwest::Url;

use crate::Route;

/// One entry of the ordered list CFNetwork answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    Direct,
    /// An HTTP proxy; for an `https` URL, one that tunnels with `CONNECT`.
    Http {
        host: String,
        port: u16,
        credentials: Option<(String, String)>,
    },
    /// A SOCKS proxy: the manual setting, or a PAC's `SOCKS` / `SOCKS4` / `SOCKS5` answer, which
    /// CFNetwork reports alike, with no version.
    Socks {
        host: String,
        port: u16,
        credentials: Option<(String, String)>,
    },
    /// A PAC file to evaluate for this URL.
    AutoConfigUrl(String),
    /// A PAC script given inline (`ProxyAutoConfigJavaScript`).
    AutoConfigScript(String),
    /// A proxy type Cmdr can't speak (FTP), named for the log.
    Unsupported(String),
}

/// Where a PAC script comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum PacSource {
    Url(String),
    Script(String),
}

/// Runs a PAC script for one URL. `None` when it couldn't (unreachable, broken, too slow): the
/// walk then moves on to the next entry, which is CFNetwork's own DIRECT fallback.
pub(crate) trait Pac {
    fn evaluate(&self, source: &PacSource, url: &Url) -> Option<Vec<Entry>>;
}

/// The first entry Cmdr can act on, the way CFNetwork clients walk the list: an unusable entry
/// is skipped, a PAC entry stands for the PAC's own answer, and an empty or exhausted list means
/// direct.
pub(crate) fn first_route(entries: &[Entry], url: &Url, pac: &dyn Pac) -> Route {
    for entry in entries {
        let source = match entry {
            Entry::AutoConfigUrl(pac_url) => PacSource::Url(pac_url.clone()),
            Entry::AutoConfigScript(script) => PacSource::Script(script.clone()),
            other => match usable(other) {
                Some(route) => return route,
                None => continue,
            },
        };
        // A PAC's answer never names another PAC, so its list is walked without one.
        if let Some(route) = pac
            .evaluate(&source, url)
            .and_then(|answer| answer.iter().find_map(usable))
        {
            return route;
        }
    }
    Route::Direct
}

/// The route one entry gives on its own, or `None` when Cmdr can't use it.
fn usable(entry: &Entry) -> Option<Route> {
    match entry {
        Entry::Direct => Some(Route::Direct),
        Entry::Http {
            host,
            port,
            credentials,
        } => Some(Route::Proxy(proxy_url("http", host, *port, credentials.as_ref()))),
        // ❗ `socks5h`: the proxy resolves the name. A network that forces traffic through a
        // SOCKS proxy often can't resolve outside names locally, and a local lookup would tell
        // the network's DNS every host Cmdr talks to. CFNetwork's SOCKS entry names no version;
        // macOS's own stack speaks SOCKS5 to it and sends the name too (`DETAILS.md` § "SOCKS").
        Entry::Socks {
            host,
            port,
            credentials,
        } => Some(Route::Proxy(proxy_url("socks5h", host, *port, credentials.as_ref()))),
        Entry::AutoConfigUrl(_) | Entry::AutoConfigScript(_) => None,
        Entry::Unsupported(kind) => {
            log::debug!(target: "cmdr_http", "skipping a {kind} proxy: Cmdr speaks HTTP and SOCKS proxies only");
            None
        }
    }
}

fn proxy_url(scheme: &str, host: &str, port: u16, credentials: Option<&(String, String)>) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    match credentials {
        Some((user, password)) => format!(
            "{scheme}://{}:{}@{host}:{port}",
            percent_encode(user),
            percent_encode(password)
        ),
        None => format!("{scheme}://{host}:{port}"),
    }
}

/// Every byte but RFC 3986's unreserved ones as `%XX`, safe in a URL's userinfo. ❗ Not
/// `form_urlencoded`: its `+` for a space comes back from hyper-util's percent-decoding as a `+`.
fn percent_encode(part: &str) -> String {
    part.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(byte).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Reading CFNetwork's CF objects into [`Entry`]s.
#[cfg(target_os = "macos")]
pub(crate) mod cf {
    use objc2_cf_network::{
        kCFProxyAutoConfigurationJavaScriptKey, kCFProxyAutoConfigurationURLKey, kCFProxyHostNameKey,
        kCFProxyPasswordKey, kCFProxyPortNumberKey, kCFProxyTypeAutoConfigurationJavaScript,
        kCFProxyTypeAutoConfigurationURL, kCFProxyTypeHTTP, kCFProxyTypeHTTPS, kCFProxyTypeKey, kCFProxyTypeNone,
        kCFProxyTypeSOCKS, kCFProxyUsernameKey,
    };
    use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL};

    use super::Entry;

    pub(crate) fn cf_url(url: &str) -> Option<CFRetained<CFURL>> {
        CFURL::from_string(None, &CFString::from_str(url), None)
    }

    /// Reads a CFNetwork proxy list: the answer to `CFNetworkCopyProxiesForURL`, or a PAC's.
    pub(crate) fn entries_from(list: &CFArray) -> Vec<Entry> {
        // SAFETY: CFNetwork documents every element of a proxy list as a CFDictionary keyed by the
        // `kCFProxy*Key` strings.
        let list: &CFArray<CFDictionary<CFString, CFType>> = unsafe { list.cast_unchecked() };
        list.iter().map(|entry| entry_from(&entry)).collect()
    }

    /// CFNetwork's key and type-name constants, read once per entry.
    struct Names {
        type_key: &'static CFString,
        host: &'static CFString,
        port: &'static CFString,
        user: &'static CFString,
        password: &'static CFString,
        pac_url: &'static CFString,
        pac_script: &'static CFString,
        none: &'static CFString,
        http: &'static CFString,
        https: &'static CFString,
        socks: &'static CFString,
        auto_config_url: &'static CFString,
        auto_config_script: &'static CFString,
    }

    fn names() -> Names {
        // SAFETY: CFNetwork exports these as immutable CFString constants that live for the whole
        // process; reading one is a plain load of a pointer nothing ever writes.
        unsafe {
            Names {
                type_key: kCFProxyTypeKey,
                host: kCFProxyHostNameKey,
                port: kCFProxyPortNumberKey,
                user: kCFProxyUsernameKey,
                password: kCFProxyPasswordKey,
                pac_url: kCFProxyAutoConfigurationURLKey,
                pac_script: kCFProxyAutoConfigurationJavaScriptKey,
                none: kCFProxyTypeNone,
                http: kCFProxyTypeHTTP,
                https: kCFProxyTypeHTTPS,
                socks: kCFProxyTypeSOCKS,
                auto_config_url: kCFProxyTypeAutoConfigurationURL,
                auto_config_script: kCFProxyTypeAutoConfigurationJavaScript,
            }
        }
    }

    fn entry_from(entry: &CFDictionary<CFString, CFType>) -> Entry {
        let names = names();
        let string = |key: &CFString| {
            entry
                .get(key)
                .and_then(|value| value.downcast::<CFString>().ok())
                .map(|value| value.to_string())
        };
        let kind = string(names.type_key).unwrap_or_default();
        let is = |constant: &CFString| kind == constant.to_string();
        if is(names.none) {
            Entry::Direct
        } else if is(names.http) || is(names.https) || is(names.socks) {
            let socks = is(names.socks);
            let port = entry
                .get(names.port)
                .and_then(|value| value.downcast::<CFNumber>().ok())
                .and_then(|value| value.as_i64())
                .and_then(|value| u16::try_from(value).ok())
                .unwrap_or(if socks { 1080 } else { 80 });
            let credentials = string(names.user).zip(string(names.password));
            match (string(names.host), socks) {
                (Some(host), false) => Entry::Http {
                    host,
                    port,
                    credentials,
                },
                (Some(host), true) => Entry::Socks {
                    host,
                    port,
                    credentials,
                },
                (None, _) => Entry::Unsupported(format!("host-less {kind}")),
            }
        } else if is(names.auto_config_url) {
            let pac = entry
                .get(names.pac_url)
                .and_then(|value| value.downcast::<CFURL>().ok())
                .map(|value| value.string().to_string());
            pac.map_or_else(
                || Entry::Unsupported(String::from("URL-less PAC")),
                Entry::AutoConfigUrl,
            )
        } else if is(names.auto_config_script) {
            string(names.pac_script).map_or_else(
                || Entry::Unsupported(String::from("script-less PAC")),
                Entry::AutoConfigScript,
            )
        } else {
            Entry::Unsupported(kind)
        }
    }
}

#[cfg(test)]
#[path = "entries_test.rs"]
mod tests;
