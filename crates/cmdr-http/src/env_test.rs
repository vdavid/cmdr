use super::*;

fn env(vars: &[(&str, &str)]) -> EnvProxies {
    EnvProxies::from_vars(|name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()))
}

fn route(env: &EnvProxies, url: &str) -> Option<Route> {
    let url = Url::parse(url).expect("a valid test URL");
    let host = url.host().expect("a test URL with a host");
    env.route(&url, &host)
}

fn proxy(url: &str) -> Option<Route> {
    Some(Route::Proxy(url.to_string()))
}

#[test]
fn an_empty_environment_has_no_say() {
    assert_eq!(route(&env(&[]), "https://example.com"), None);
    assert_eq!(route(&env(&[("HTTPS_PROXY", "  ")]), "https://example.com"), None);
}

#[test]
fn each_scheme_reads_its_own_variable_uppercase_first() {
    let env = env(&[
        ("HTTP_PROXY", "http://upper:1"),
        ("http_proxy", "http://lower:2"),
        ("https_proxy", "http://lower:3"),
    ]);
    assert_eq!(route(&env, "http://example.com"), proxy("http://upper:1"));
    assert_eq!(route(&env, "https://example.com"), proxy("http://lower:3"));
}

#[test]
fn all_proxy_fills_a_scheme_left_unset() {
    let env = env(&[("ALL_PROXY", "all.proxy:1080"), ("HTTPS_PROXY", "https.proxy:443")]);
    assert_eq!(route(&env, "http://example.com"), proxy("http://all.proxy:1080"));
    assert_eq!(route(&env, "https://example.com"), proxy("http://https.proxy:443"));
}

#[test]
fn credentials_in_the_variable_are_kept() {
    let env = env(&[("HTTPS_PROXY", "http://ada:secret@proxy.corp:3128")]);
    assert_eq!(
        route(&env, "https://example.com"),
        proxy("http://ada:secret@proxy.corp:3128")
    );
}

#[test]
fn a_socks_variable_keeps_its_scheme_and_so_its_dns_choice() {
    // `socks5` resolves names on this Mac, `socks5h` at the proxy: curl's convention, kept as typed.
    let env = env(&[
        ("ALL_PROXY", "socks5h://ada:secret@socks.corp:1080"),
        ("HTTP_PROXY", "socks5://socks.corp"),
    ]);
    assert_eq!(
        route(&env, "https://example.com"),
        proxy("socks5h://ada:secret@socks.corp:1080")
    );
    assert_eq!(route(&env, "http://example.com"), proxy("socks5://socks.corp"));
}

#[test]
fn no_proxy_matches_domains_and_their_subdomains() {
    let env = env(&[
        ("HTTPS_PROXY", "http://p:1"),
        ("NO_PROXY", "corp.example, .internal.test,*.wild.test"),
    ]);
    for host in [
        "corp.example",
        "files.corp.example",
        "a.internal.test",
        "internal.test",
        "x.wild.test",
        "CORP.EXAMPLE.",
    ] {
        assert_eq!(route(&env, &format!("https://{host}/")), Some(Route::Direct), "{host}");
    }
    for host in ["notcorp.example", "example.com", "corp.example.evil.test"] {
        assert_eq!(route(&env, &format!("https://{host}/")), proxy("http://p:1"), "{host}");
    }
}

#[test]
fn no_proxy_matches_addresses_and_networks() {
    let env = env(&[
        ("HTTPS_PROXY", "http://p:1"),
        ("NO_PROXY", "10.0.0.0/8,192.168.1.20,fd00::/8,[2001:db8::1]"),
    ]);
    for url in [
        "https://10.1.2.3/",
        "https://192.168.1.20/",
        "https://[fd12::1]/",
        "https://[2001:db8::1]/",
    ] {
        assert_eq!(route(&env, url), Some(Route::Direct), "{url}");
    }
    for url in ["https://11.0.0.1/", "https://192.168.1.21/", "https://[2001:db8::2]/"] {
        assert_eq!(route(&env, url), proxy("http://p:1"), "{url}");
    }
}

#[test]
fn no_proxy_star_matches_everything_and_ports_are_ignored() {
    assert_eq!(
        route(
            &env(&[("HTTPS_PROXY", "http://p:1"), ("NO_PROXY", "*")]),
            "https://example.com"
        ),
        Some(Route::Direct)
    );
    let env = env(&[("HTTPS_PROXY", "http://p:1"), ("no_proxy", "example.com:8443")]);
    assert_eq!(route(&env, "https://example.com:9999/"), Some(Route::Direct));
}
