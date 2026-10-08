//! The real CFNetwork call against settings dictionaries the tests build, so they never read or
//! change this Mac's own settings.

use std::io::{Read, Write};
use std::net::TcpListener;

use objc2_cf_network::{
    kCFNetworkProxiesExceptionsList, kCFNetworkProxiesExcludeSimpleHostnames, kCFNetworkProxiesHTTPEnable,
    kCFNetworkProxiesHTTPPort, kCFNetworkProxiesHTTPProxy, kCFNetworkProxiesHTTPSEnable, kCFNetworkProxiesHTTPSPort,
    kCFNetworkProxiesHTTPSProxy, kCFNetworkProxiesProxyAutoConfigEnable, kCFNetworkProxiesProxyAutoConfigURLString,
    kCFNetworkProxiesSOCKSEnable, kCFNetworkProxiesSOCKSPort, kCFNetworkProxiesSOCKSProxy,
};
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString};
use reqwest::Url;

use super::mac::{entries_for, settings};
use crate::Route;
use crate::entries::{Entry, Pac, PacSource, first_route};
use crate::pac::{MacPac, TIMEOUT};

/// A PAC evaluator for the tests whose settings name no PAC.
struct NoPac;

impl Pac for NoPac {
    fn evaluate(&self, _source: &PacSource, _url: &Url) -> Option<Vec<Entry>> {
        None
    }
}

/// A Mac with a manual HTTP and HTTPS proxy, a bypass list, and "Exclude simple hostnames" on.
fn static_proxy_settings() -> CFRetained<CFDictionary> {
    let on = CFNumber::new_i32(1);
    let port = CFNumber::new_i32(3128);
    let host = CFString::from_str("proxy.test");
    let exceptions = CFArray::<CFString>::from_retained_objects(&[
        CFString::from_str("*.corp.test"),
        CFString::from_str("getcmdr.com"),
    ]);
    // SAFETY: CFNetwork's key constants are immutable, process-lifetime CFStrings.
    let keys = unsafe {
        [
            kCFNetworkProxiesHTTPEnable,
            kCFNetworkProxiesHTTPProxy,
            kCFNetworkProxiesHTTPPort,
            kCFNetworkProxiesHTTPSEnable,
            kCFNetworkProxiesHTTPSProxy,
            kCFNetworkProxiesHTTPSPort,
            kCFNetworkProxiesExceptionsList,
            kCFNetworkProxiesExcludeSimpleHostnames,
        ]
    };
    settings(&[
        (keys[0], &on),
        (keys[1], &host),
        (keys[2], &port),
        (keys[3], &on),
        (keys[4], &host),
        (keys[5], &port),
        (keys[6], &exceptions),
        (keys[7], &on),
    ])
}

/// A Mac with only "SOCKS proxy" on, at `socks.test:1080`, plus `extra` settings.
fn socks_settings(extra: &[(&CFString, &objc2_core_foundation::CFType)]) -> CFRetained<CFDictionary> {
    let on = CFNumber::new_i32(1);
    let port = CFNumber::new_i32(1080);
    let host = CFString::from_str("socks.test");
    // SAFETY: CFNetwork's key constants are immutable, process-lifetime CFStrings.
    let keys = unsafe {
        [
            kCFNetworkProxiesSOCKSEnable,
            kCFNetworkProxiesSOCKSProxy,
            kCFNetworkProxiesSOCKSPort,
        ]
    };
    let mut pairs: Vec<(&CFString, &objc2_core_foundation::CFType)> =
        vec![(keys[0], &on), (keys[1], &host), (keys[2], &port)];
    pairs.extend_from_slice(extra);
    settings(&pairs)
}

fn socks_entry() -> Entry {
    Entry::Socks {
        host: "socks.test".into(),
        port: 1080,
        credentials: None,
    }
}

/// A Mac set to "Automatic proxy configuration" with `pac_url`.
fn pac_settings(pac_url: &str) -> CFRetained<CFDictionary> {
    let on = CFNumber::new_i32(1);
    let pac = CFString::from_str(pac_url);
    // SAFETY: CFNetwork's key constants are immutable, process-lifetime CFStrings.
    let (enable, url) = unsafe {
        (
            kCFNetworkProxiesProxyAutoConfigEnable,
            kCFNetworkProxiesProxyAutoConfigURLString,
        )
    };
    settings(&[(enable, &on), (url, &pac)])
}

fn parse(url: &str) -> Url {
    Url::parse(url).expect("a valid test URL")
}

fn route(settings: &CFDictionary, url: &str) -> Route {
    let url = parse(url);
    first_route(&entries_for(settings, &url), &url, &NoPac)
}

#[test]
fn a_manual_proxy_carries_ordinary_hosts() {
    let settings = static_proxy_settings();
    assert_eq!(
        route(&settings, "https://example.com"),
        Route::Proxy("http://proxy.test:3128".into())
    );
    assert_eq!(
        route(&settings, "http://example.com"),
        Route::Proxy("http://proxy.test:3128".into())
    );
}

#[test]
fn the_bypass_list_sends_its_hosts_direct() {
    let settings = static_proxy_settings();
    assert_eq!(route(&settings, "https://getcmdr.com"), Route::Direct);
    assert_eq!(route(&settings, "https://files.corp.test"), Route::Direct);
}

#[test]
fn exclude_simple_hostnames_sends_dotless_hosts_direct() {
    assert_eq!(route(&static_proxy_settings(), "http://intranet"), Route::Direct);
}

#[test]
fn a_manual_socks_proxy_carries_both_schemes() {
    let settings = socks_settings(&[]);
    assert_eq!(
        entries_for(&settings, &parse("https://example.com")),
        vec![socks_entry()]
    );
    for url in ["https://example.com", "http://example.com"] {
        assert_eq!(
            route(&settings, url),
            Route::Proxy("socks5h://socks.test:1080".into()),
            "{url}"
        );
    }
}

#[test]
fn an_http_proxy_for_the_scheme_comes_before_the_socks_proxy() {
    let on = CFNumber::new_i32(1);
    let port = CFNumber::new_i32(3128);
    let host = CFString::from_str("proxy.test");
    // SAFETY: CFNetwork's key constants are immutable, process-lifetime CFStrings.
    let keys = unsafe {
        [
            kCFNetworkProxiesHTTPSEnable,
            kCFNetworkProxiesHTTPSProxy,
            kCFNetworkProxiesHTTPSPort,
        ]
    };
    let settings = socks_settings(&[(keys[0], &on), (keys[1], &host), (keys[2], &port)]);
    assert_eq!(
        route(&settings, "https://example.com"),
        Route::Proxy("http://proxy.test:3128".into())
    );
    // No HTTP proxy for plain `http`, so the SOCKS proxy carries it.
    assert_eq!(
        route(&settings, "http://example.com"),
        Route::Proxy("socks5h://socks.test:1080".into())
    );
}

#[test]
fn no_proxy_settings_mean_direct() {
    assert_eq!(route(&settings(&[]), "https://example.com"), Route::Direct);
}

#[test]
fn a_pac_setting_surfaces_as_its_url() {
    let settings = pac_settings("http://127.0.0.1:1/proxy.pac");
    // CFNetwork appends its own DIRECT fallback after the PAC entry (verified on macOS 27.0).
    assert_eq!(
        entries_for(&settings, &parse("https://example.com")),
        vec![
            Entry::AutoConfigUrl("http://127.0.0.1:1/proxy.pac".into()),
            Entry::Direct
        ]
    );
}

#[test]
fn a_pac_setting_routes_through_the_pacs_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let address = listener.local_addr().expect("a bound address");
    std::thread::spawn(move || {
        let body = r#"function FindProxyForURL(url, host) { return host == "example.com" ? "PROXY proxy.test:3128" : "DIRECT"; }"#;
        for mut stream in listener.incoming().flatten() {
            let _ = stream.read(&mut [0u8; 2048]);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    let settings = pac_settings(&format!("http://{address}/proxy.pac"));
    let pac = MacPac::new(TIMEOUT);
    let walk = |url: &str| {
        let url = parse(url);
        first_route(&entries_for(&settings, &url), &url, &pac)
    };
    assert_eq!(
        walk("https://example.com"),
        Route::Proxy("http://proxy.test:3128".into())
    );
    assert_eq!(walk("https://other.test"), Route::Direct);
}
