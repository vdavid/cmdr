//! Complete remote references and name-derived IDs as they appear in diagnostics.
//!
//! This is deliberately a display-boundary parser. It neither changes the functional URL/ID
//! vocabulary nor tries to infer arbitrary hyphenated strings.

use super::RedactionContext;
use super::context::TokenDomain;
use super::identity_token;
use super::paths::{has_extension_like_suffix, redact_leaf};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub(super) fn redact_remote_url(reference: &str, context: Option<&RedactionContext>) -> String {
    if is_svelte_error_url(reference) {
        return reference.to_string();
    }
    let Some((scheme, remainder)) = reference.split_once("://") else {
        return reference.to_string();
    };
    // The standards parser is authoritative for valid host syntax. The bounded fallback below
    // intentionally keeps working when logs contain a recognizable but rejected spelling.
    let parsed = url::Url::parse(reference).ok();
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    let suffix = &remainder[authority_end..];
    let (userinfo, host_port) = authority
        .rsplit_once('@')
        .map_or((None, authority), |(user, host)| (Some(user), host));

    let mut out = format!("{scheme}://");
    if let Some(userinfo) = userinfo {
        out.push_str(&redact_userinfo(userinfo, context));
        out.push('@');
    }
    out.push_str(&redact_host_port(host_port, parsed.as_ref(), context));
    out.push_str(&redact_remote_suffix(
        suffix,
        scheme.eq_ignore_ascii_case("smb"),
        context,
    ));
    out
}

/// A production Svelte error, `https://svelte.dev/e/<snake_case_code>`, is the whole message an
/// uncaught frontend error carries, and it names a public error code rather than anyone's data.
/// Only that exact shape passes: a port, userinfo, query, fragment, or extra segment means it
/// isn't one Svelte built, and it gets the full remote-URL treatment.
fn is_svelte_error_url(reference: &str) -> bool {
    reference.strip_prefix("https://svelte.dev/e/").is_some_and(|code| {
        code.starts_with(|c: char| c.is_ascii_lowercase())
            && code
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    })
}

pub(super) fn redact_scheme_less(reference: &str, context: Option<&RedactionContext>) -> String {
    let Some(remainder) = reference.strip_prefix("//") else {
        return reference.to_string();
    };
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    let suffix = &remainder[authority_end..];
    let (userinfo, host_port) = authority
        .rsplit_once('@')
        .map_or((None, authority), |(user, host)| (Some(user), host));

    let mut out = "//".to_string();
    if let Some(userinfo) = userinfo {
        out.push_str(&redact_userinfo(userinfo, context));
        out.push('@');
    }
    out.push_str(&redact_host_port(host_port, None, context));
    out.push_str(&redact_remote_suffix(suffix, true, context));
    out
}

pub(super) fn redact_remote_unc(reference: &str, context: Option<&RedactionContext>) -> String {
    let Some(remainder) = reference.strip_prefix(r"\\") else {
        return reference.to_string();
    };
    let mut parts = remainder.split('\\');
    let authority = parts.next().unwrap_or_default();
    let mut out = format!(r"\\{}", redact_host_port(authority, None, context));
    if let Some(share) = parts.next() {
        out.push('\\');
        out.push_str(&identity_token(
            "share",
            TokenDomain::Volume,
            &decode_identity(share),
            context,
        ));
    }
    let tail: Vec<&str> = parts.collect();
    if !tail.is_empty() {
        out.push('\\');
        out.push_str(&redact_segments(&tail, '\\', context));
    }
    out
}

pub(super) fn redact_derived_id(id: &str, context: Option<&RedactionContext>) -> String {
    let (core, storage) = id
        .rsplit_once(':')
        .filter(|(core, storage)| {
            cmdr_fs::volume::VolumeScheme::of(core) == cmdr_fs::volume::VolumeScheme::Mtp
                && storage.parse::<u32>().is_ok_and(|value| value.to_string() == *storage)
        })
        .map_or((id, None), |(core, storage)| (core, Some(storage)));
    let Some((scheme_and_slug, digest)) = core.rsplit_once('-') else {
        return id.to_string();
    };
    let (scheme, slug) = scheme_and_slug.split_once('-').unwrap_or((scheme_and_slug, ""));
    if !matches!(
        scheme,
        "smb" | "sftp" | "webdav" | "s3" | "adb" | "mtp" | "vol" | "path"
    ) || !valid_derived_slug(slug)
        || digest.len() != 16
        || !digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        || (id.contains(':') && storage.is_none())
    {
        return id.to_string();
    }
    let (kind, domain) = match scheme {
        "adb" | "mtp" => ("device-id", TokenDomain::DeviceId),
        _ => ("volume-id", TokenDomain::VolumeId),
    };
    let mut out = format!("{scheme}-{}", identity_token(kind, domain, core, context));
    if let Some(storage) = storage {
        out.push(':');
        out.push_str(storage);
    }
    out
}

pub(super) fn redact_manual_server_id(id: &str, context: Option<&RedactionContext>) -> String {
    let remainder = id.strip_prefix("manual-").unwrap_or(id);
    let (server, port) = remainder.rsplit_once('-').unwrap_or((remainder, ""));
    if server.is_empty() || port.parse::<u16>().map_or(true, |value| value.to_string() != port) {
        return id.to_string();
    }
    format!(
        "manual-{}-{port}",
        identity_token("server-id", TokenDomain::ServerId, server, context)
    )
}

fn valid_derived_slug(slug: &str) -> bool {
    slug.chars().count() <= 24
        && (slug.is_empty()
            || slug
                .split('-')
                .all(|part| !part.is_empty() && part.chars().all(char::is_alphanumeric)))
}

pub(super) fn trim_reference_end(reference: &str) -> &str {
    reference.trim_end_matches(['.', ',', ';', '!', ')', ']', '}'])
}

fn redact_userinfo(userinfo: &str, context: Option<&RedactionContext>) -> String {
    let (username, password) = userinfo
        .split_once(':')
        .map_or((userinfo, None), |(user, password)| (user, Some(password)));
    let mut out = identity_token("user", TokenDomain::Userinfo, &decode_identity(username), context);
    if let Some(password) = password {
        out.push(':');
        out.push_str(&identity_token(
            "credential",
            TokenDomain::Credential,
            &decode_identity(password),
            context,
        ));
    }
    out
}

fn redact_host_port(host_port: &str, parsed: Option<&url::Url>, context: Option<&RedactionContext>) -> String {
    let (host, port, bracketed) = if let Some(after_open) = host_port.strip_prefix('[') {
        let Some(close) = after_open.find(']') else {
            return format!("[{}", redact_host(after_open, context));
        };
        {
            let host = &after_open[..close];
            let port = after_open[close + 1..].strip_prefix(':').unwrap_or("");
            (host, port, true)
        }
    } else if let Some((host, port)) = host_port
        .rsplit_once(':')
        .filter(|(_, port)| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
    {
        (host, port, false)
    } else {
        (host_port, "", false)
    };

    // For a valid URL, a disagreement means the fallback split mistook an IPv6 group for a
    // port. Standard IPv6 authorities are bracketed, but this also protects future syntax.
    let (host, port) = parsed
        .filter(|_| !bracketed)
        .and_then(|url| url.host_str().map(|parsed_host| (parsed_host, url.port())))
        .filter(|(parsed_host, _)| host.contains(':') && *parsed_host == host_port)
        .map_or((host, port), |(parsed_host, _)| (parsed_host, ""));
    let redacted = redact_host(host, context);
    let mut out = if bracketed { format!("[{redacted}]") } else { redacted };
    if !port.is_empty() {
        out.push(':');
        out.push_str(port);
    }
    out
}

pub(super) fn redact_host(host: &str, context: Option<&RedactionContext>) -> String {
    if host.is_empty() {
        return String::new();
    }
    let decoded = decode_identity(host);
    // A fully qualified `.local.` keeps its root dot.
    let (decoded, root_dot) = decoded
        .strip_suffix('.')
        .map_or((decoded.as_str(), false), |host| (host, true));
    let (identity, local) = decoded
        .strip_suffix(".local")
        .or_else(|| decoded.strip_suffix(".LOCAL"))
        .map_or((decoded, false), |host| (host, true));
    let folded = identity.to_lowercase();
    // A Bonjour instance (`naspolya._smb._tcp`): the device's own name is the identity, and the
    // service type after it is public, so `server="naspolya"` and the instance share a token.
    let (folded, service) = match bonjour_service_split(&folded) {
        Some((instance, service)) => (instance.to_string(), service),
        None => (folded.clone(), String::new()),
    };
    let kind = folded.parse::<IpAddr>().map_or("host", |address| address_kind(address));
    let mut out = identity_token(kind, TokenDomain::Host, &folded, context);
    out.push_str(&service);
    if local {
        out.push_str(".local");
    }
    if root_dot {
        out.push('.');
    }
    out
}

/// Split `instance._svc._tcp` into (`instance`, `._svc._tcp`); `None` for anything else.
fn bonjour_service_split(host: &str) -> Option<(&str, String)> {
    let without_proto = host.strip_suffix("._tcp").or_else(|| host.strip_suffix("._udp"))?;
    let proto = &host[without_proto.len()..];
    let (instance, service) = without_proto.rsplit_once("._")?;
    let valid_service = !service.is_empty() && service.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (valid_service && !instance.is_empty()).then(|| (instance, format!("._{service}{proto}")))
}

pub(super) fn redact_mdns_host(host: &str, context: Option<&RedactionContext>) -> String {
    redact_host(host, context)
}

fn address_kind(address: IpAddr) -> &'static str {
    match address {
        IpAddr::V4(address) => ipv4_kind(address),
        IpAddr::V6(address) => ipv6_kind(address),
    }
}

fn ipv4_kind(address: Ipv4Addr) -> &'static str {
    if address.is_loopback() {
        "ipv4-loopback"
    } else if address.is_private() {
        "ipv4-private"
    } else if address.is_link_local() {
        "ipv4-link-local"
    } else if address.is_unspecified() {
        "ipv4-unspecified"
    } else {
        "ipv4-public"
    }
}

fn ipv6_kind(address: Ipv6Addr) -> &'static str {
    let first = address.segments()[0];
    if address.is_loopback() {
        "ipv6-loopback"
    } else if address.is_unspecified() {
        "ipv6-unspecified"
    } else if first & 0xfe00 == 0xfc00 {
        "ipv6-private"
    } else if first & 0xffc0 == 0xfe80 {
        "ipv6-link-local"
    } else {
        "ipv6-public"
    }
}

fn redact_remote_suffix(suffix: &str, smb: bool, context: Option<&RedactionContext>) -> String {
    let (before_fragment, fragment) = suffix
        .split_once('#')
        .map_or((suffix, None), |(left, right)| (left, Some(right)));
    let (path, query) = before_fragment
        .split_once('?')
        .map_or((before_fragment, None), |(left, right)| (left, Some(right)));
    let mut out = redact_remote_path(path, smb, context);
    if let Some(query) = query {
        out.push('?');
        out.push_str(&redact_query(query, context));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&identity_token(
            "fragment",
            TokenDomain::Fragment,
            &decode_identity(fragment),
            context,
        ));
    }
    out
}

fn redact_remote_path(path: &str, smb: bool, context: Option<&RedactionContext>) -> String {
    if path.is_empty() {
        return String::new();
    }
    let leading = path.starts_with('/');
    let trailing = path.ends_with('/') && path.len() > 1;
    let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
    let mut out = if leading { "/".to_string() } else { String::new() };
    if smb && !segments.is_empty() {
        out.push_str(&identity_token(
            "share",
            TokenDomain::Volume,
            &decode_identity(segments[0]),
            context,
        ));
        if segments.len() > 1 {
            out.push('/');
            out.push_str(&redact_segments(&segments[1..], '/', context));
        }
    } else {
        out.push_str(&redact_segments(&segments, '/', context));
    }
    if trailing {
        out.push('/');
    }
    out
}

fn redact_segments(segments: &[&str], separator: char, context: Option<&RedactionContext>) -> String {
    let mut out = String::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            out.push(separator);
        }
        let decoded = decode_identity(segment);
        if index + 1 == segments.len() && has_extension_like_suffix(&decoded) {
            out.push_str(&redact_leaf(&decoded, true, context));
        } else {
            out.push_str(&identity_token("dir", TokenDomain::Path, &decoded, context));
        }
    }
    out
}

fn redact_query(query: &str, context: Option<&RedactionContext>) -> String {
    query
        .split('&')
        .map(|pair| {
            let (key, value) = pair
                .split_once('=')
                .map_or((pair, None), |(key, value)| (key, Some(value)));
            let mut out = identity_token("query", TokenDomain::Query, &decode_identity(key), context);
            if let Some(value) = value {
                out.push('=');
                out.push_str(&identity_token(
                    "query",
                    TokenDomain::Query,
                    &decode_identity(value),
                    context,
                ));
            }
            out
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn decode_identity(value: &str) -> String {
    urlencoding::decode(value).map_or_else(|_| value.to_string(), |decoded| decoded.into_owned())
}
