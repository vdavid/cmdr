//! `AllowedCloudAIHosts` entries: where cloud AI may send data. Entry and request URL go through
//! the same normalization and compare as `url::Host` values. Rules: `DETAILS.md` § Key catalog.

use std::fmt;
use std::net::Ipv6Addr;

use url::{Host, Url};

/// One allowed destination: an exact host, or every subdomain of a domain (`*.openai.azure.com`),
/// optionally pinned to one port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPattern {
    host: HostMatch,
    /// `None` matches any port.
    port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostMatch {
    Exact(Host<String>),
    /// Any subdomain of this domain, never the domain itself.
    Subdomains(String),
}

impl HostPattern {
    /// A bare host, `host:port`, a `*.` pattern, or a pasted URL (only its host and an explicit
    /// port count). `None` for anything malformed: an empty entry, `*`, `*.`, a pattern on an IP,
    /// or a `*` anywhere but the leading label.
    pub fn parse(entry: &str) -> Option<Self> {
        let entry = entry.trim();
        let (host_text, port) = if entry.contains("://") {
            // `host_str()` keeps IPv6 brackets and a `*` label (the URL parser allows `*` in a
            // domain), so the shared host parse below sees what a bare entry would give it.
            let url = Url::parse(entry).ok()?;
            (url.host_str()?.to_string(), url.port())
        } else {
            let (host, port) = split_host_port(entry)?;
            (host.to_string(), port)
        };

        let host = match host_text.strip_prefix("*.") {
            Some(suffix) => match parse_host(suffix)? {
                Host::Domain(domain) => HostMatch::Subdomains(domain),
                Host::Ipv4(_) | Host::Ipv6(_) => return None,
            },
            None => HostMatch::Exact(parse_host(&host_text)?),
        };
        Some(Self { host, port })
    }

    /// Whether a request to `url` may go out under this entry.
    pub fn allows(&self, url: &Url) -> bool {
        let Some(host) = url.host().and_then(|host| normalize(host.to_owned())) else {
            return false;
        };
        if self.port.is_some() && self.port != url.port_or_known_default() {
            return false;
        }
        match (&self.host, &host) {
            (HostMatch::Exact(allowed), _) => *allowed == host,
            (HostMatch::Subdomains(suffix), Host::Domain(domain)) => domain
                .strip_suffix(suffix.as_str())
                .is_some_and(|label| label.len() > 1 && label.ends_with('.')),
            (HostMatch::Subdomains(_), Host::Ipv4(_) | Host::Ipv6(_)) => false,
        }
    }
}

/// Splits a bare entry into host and optional port: `host`, `host:port`, `::1`, `[::1]`,
/// `[::1]:port`. `None` for an unparseable port or an unclosed bracket.
fn split_host_port(entry: &str) -> Option<(&str, Option<u16>)> {
    if entry.parse::<Ipv6Addr>().is_ok() {
        return Some((entry, None));
    }
    if entry.starts_with('[') {
        let close = entry.find(']')?;
        let (host, rest) = entry.split_at(close + 1);
        return match rest {
            "" => Some((host, None)),
            _ => Some((host, Some(rest.strip_prefix(':')?.parse().ok()?))),
        };
    }
    match entry.rsplit_once(':') {
        Some((host, port)) => Some((host, Some(port.parse().ok()?))),
        None => Some((entry, None)),
    }
}

/// One host spelling to a comparable `url::Host`: IDNA to punycode, lowercase, IPs parsed, one
/// trailing dot dropped. `None` for an empty host or a `*` anywhere in it.
fn parse_host(text: &str) -> Option<Host<String>> {
    if text.contains('*') {
        return None;
    }
    if let Ok(v6) = text.parse::<Ipv6Addr>() {
        return Some(Host::Ipv6(v6));
    }
    normalize(Host::parse(text).ok()?)
}

/// Drops one trailing dot from a domain (`api.openai.com.` is the same host), which neither the
/// URL parser nor `Host::parse` does.
fn normalize(host: Host<String>) -> Option<Host<String>> {
    match host {
        Host::Domain(domain) => {
            let domain = domain.strip_suffix('.').unwrap_or(&domain);
            (!domain.is_empty()).then(|| Host::Domain(domain.to_string()))
        }
        ip => Some(ip),
    }
}

impl fmt::Display for HostPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.host {
            HostMatch::Exact(host) => write!(f, "{host}")?,
            HostMatch::Subdomains(domain) => write!(f, "*.{domain}")?,
        }
        match self.port {
            Some(port) => write!(f, ":{port}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(entry: &str) -> HostPattern {
        HostPattern::parse(entry).unwrap_or_else(|| panic!("{entry:?} should parse"))
    }

    fn allows(entry: &str, url: &str) -> bool {
        pattern(entry).allows(&Url::parse(url).expect("a valid test URL"))
    }

    #[test]
    fn an_exact_host_matches_only_itself_on_any_port() {
        assert!(allows("api.openai.com", "https://api.openai.com/v1"));
        assert!(allows("api.openai.com", "http://api.openai.com:8080/v1"));
        assert!(!allows("api.openai.com", "https://openai.com/v1"));
        assert!(!allows("api.openai.com", "https://eu.api.openai.com/v1"));
    }

    #[test]
    fn a_suffix_pattern_matches_subdomains_but_not_the_apex() {
        assert!(allows("*.openai.azure.com", "https://acme.openai.azure.com/openai/v1"));
        assert!(allows("*.openai.azure.com", "https://a.b.openai.azure.com/"));
        assert!(!allows("*.openai.azure.com", "https://openai.azure.com/"));
        assert!(!allows("*.openai.azure.com", "https://evilopenai.azure.com/"));
        assert!(allows("*.com", "https://anything.com/"));
    }

    #[test]
    fn hosts_compare_case_insensitively_and_ignore_one_trailing_dot() {
        assert!(allows("API.OpenAI.com", "https://api.openai.com/"));
        assert!(allows("api.openai.com", "https://API.OPENAI.COM/"));
        assert!(allows("api.openai.com.", "https://api.openai.com/"));
        assert!(allows("api.openai.com", "https://api.openai.com./"));
        assert!(allows("*.openai.azure.com.", "https://x.openai.azure.com./"));
    }

    #[test]
    fn an_entry_with_a_port_matches_only_that_port() {
        assert!(allows("localhost:11434", "http://localhost:11434/v1"));
        assert!(!allows("localhost:11434", "http://localhost:1234/v1"));
        assert!(allows("api.openai.com:443", "https://api.openai.com/v1"));
        assert!(!allows("api.openai.com:443", "http://api.openai.com/v1"));
        assert!(allows("localhost", "http://localhost:1234/v1"));
    }

    #[test]
    fn a_pasted_url_contributes_its_host_and_explicit_port() {
        assert!(allows("https://api.openai.com/v1", "https://api.openai.com/v1/chat"));
        assert!(allows("https://api.openai.com/v1", "https://api.openai.com:8443/"));
        assert!(allows("http://localhost:11434/v1", "http://localhost:11434/"));
        assert!(!allows("http://localhost:11434/v1", "http://localhost:1234/"));
        assert!(allows(
            "https://*.openai.azure.com/openai/v1",
            "https://acme.openai.azure.com/"
        ));
    }

    #[test]
    fn ipv6_entries_match_bracketed_url_hosts() {
        assert!(allows("::1", "http://[::1]:11434/v1"));
        assert!(allows("[::1]", "http://[::1]:11434/v1"));
        assert!(allows("[::1]:11434", "http://[::1]:11434/v1"));
        assert!(!allows("[::1]:11434", "http://[::1]:1234/v1"));
        assert!(allows("127.0.0.1", "http://127.0.0.1:1234/v1"));
        assert!(!allows("127.0.0.1", "http://[::1]:1234/v1"));
    }

    #[test]
    fn userinfo_and_look_alikes_dont_fool_it() {
        assert!(!allows("api.openai.com", "https://api.openai.com@evil.com/v1"));
        assert!(!allows("api.openai.com", "https://api.openai.com.evil.com/v1"));
        assert!(!allows("*.openai.com", "https://api.openai.com.evil.com/v1"));
    }

    #[test]
    fn international_names_match_their_punycode() {
        assert!(allows("bücher.example", "https://xn--bcher-kva.example/"));
        assert!(allows("xn--bcher-kva.example", "https://bücher.example/"));
        assert!(allows("*.bücher.example", "https://api.xn--bcher-kva.example/"));
    }

    #[test]
    fn malformed_entries_dont_parse() {
        for entry in [
            "",
            "   ",
            "*",
            "*.",
            "*.127.0.0.1",
            "*.[::1]",
            "api.*.com",
            "**.openai.com",
            "api openai.com",
            "api.openai.com:notaport",
            "api.openai.com:99999",
            "api.openai.com/v1",
            "[::1",
        ] {
            assert_eq!(HostPattern::parse(entry), None, "{entry:?} must not parse");
        }
    }

    #[test]
    fn it_prints_its_normalized_form() {
        assert_eq!(pattern("API.OpenAI.com.").to_string(), "api.openai.com");
        assert_eq!(pattern("*.openai.azure.com").to_string(), "*.openai.azure.com");
        assert_eq!(pattern("localhost:11434").to_string(), "localhost:11434");
        assert_eq!(pattern("::1").to_string(), "[::1]");
        assert_eq!(pattern("https://api.openai.com/v1").to_string(), "api.openai.com");
    }
}
