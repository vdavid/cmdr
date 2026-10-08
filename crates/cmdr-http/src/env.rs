//! The `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY` environment variables, read once.
//!
//! Same names, precedence, and lowercase twins as reqwest's own reader (hyper-util's
//! `Matcher::from_env`), so a setup that worked before this crate keeps working. A GUI app rarely
//! has them (launchd doesn't pass a shell's), but `launchctl setenv` and a Terminal launch do.

use std::net::IpAddr;

use reqwest::Url;
use url::Host;

use crate::Route;

/// The proxy environment, snapshotted when the first client is built.
#[derive(Debug, Default)]
pub(crate) struct EnvProxies {
    http: Option<String>,
    https: Option<String>,
    no_proxy: NoProxy,
}

impl EnvProxies {
    pub(crate) fn from_process() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// Builds from a variable lookup; an empty value counts as unset.
    pub(crate) fn from_vars(get: impl Fn(&str) -> Option<String>) -> Self {
        let first = |names: &[&str]| {
            names.iter().find_map(|name| {
                get(name)
                    .map(|value| value.trim().to_string())
                    .filter(|v| !v.is_empty())
            })
        };
        let all = first(&["ALL_PROXY", "all_proxy"]);
        Self {
            http: first(&["HTTP_PROXY", "http_proxy"])
                .or_else(|| all.clone())
                .map(with_scheme),
            https: first(&["HTTPS_PROXY", "https_proxy"]).or(all).map(with_scheme),
            no_proxy: NoProxy::parse(&first(&["NO_PROXY", "no_proxy"]).unwrap_or_default()),
        }
    }

    /// The environment's verdict, or `None` when it has no say and the system settings decide.
    /// `NO_PROXY` wins over every proxy, the system's included, as it does in reqwest.
    pub(crate) fn route(&self, url: &Url, host: &Host<&str>) -> Option<Route> {
        if self.no_proxy.matches(host) {
            return Some(Route::Direct);
        }
        let proxy = match url.scheme() {
            "https" => self.https.as_ref(),
            "http" => self.http.as_ref(),
            _ => None,
        };
        proxy.map(|proxy| Route::Proxy(proxy.clone()))
    }
}

/// `proxy.corp:3128` means `http://proxy.corp:3128`, as in curl and reqwest.
fn with_scheme(value: String) -> String {
    if value.contains("://") {
        value
    } else {
        format!("http://{value}")
    }
}

/// A parsed `NO_PROXY`: `*`, domains (`corp.example` and `.corp.example` both cover
/// subdomains), IP addresses, and CIDR blocks. Ports in an entry are ignored, as in reqwest.
#[derive(Debug, Default)]
struct NoProxy {
    everything: bool,
    domains: Vec<String>,
    networks: Vec<(IpAddr, u8)>,
}

impl NoProxy {
    fn parse(list: &str) -> Self {
        let mut parsed = Self::default();
        for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            if entry == "*" {
                parsed.everything = true;
            } else if let Some(network) = parse_network(entry) {
                parsed.networks.push(network);
            } else {
                let domain = entry.split(':').next().unwrap_or(entry);
                let domain = domain.trim_start_matches("*.").trim_start_matches('.');
                parsed.domains.push(domain.to_ascii_lowercase());
            }
        }
        parsed
    }

    fn matches(&self, host: &Host<&str>) -> bool {
        if self.everything {
            return true;
        }
        if let Some(ip) = host_ip(host) {
            return self
                .networks
                .iter()
                .any(|&(network, prefix)| in_network(ip, network, prefix));
        }
        let Host::Domain(name) = host else { return false };
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        self.domains.iter().any(|domain| {
            name == *domain
                || name
                    .strip_suffix(domain.as_str())
                    .is_some_and(|rest| rest.ends_with('.'))
        })
    }
}

/// `host` as an IP address, when it is one.
fn host_ip(host: &Host<&str>) -> Option<IpAddr> {
    match host {
        Host::Domain(_) => None,
        Host::Ipv4(ip) => Some(IpAddr::V4(*ip)),
        Host::Ipv6(ip) => Some(IpAddr::V6(*ip)),
    }
}

/// `10.0.0.0/8`, `10.1.2.3`, `::1`, `[::1]`, or `fd00::/8`.
fn parse_network(entry: &str) -> Option<(IpAddr, u8)> {
    let (address, prefix) = match entry.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix.parse::<u8>().ok()?)),
        None => (entry, None),
    };
    let address = address.trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = address.parse().ok()?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = prefix.unwrap_or(max);
    (prefix <= max).then_some((ip, prefix))
}

fn in_network(ip: IpAddr, network: IpAddr, prefix: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
            u32::from(ip) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) => {
            let mask = u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0);
            u128::from(ip) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "env_test.rs"]
mod tests;
