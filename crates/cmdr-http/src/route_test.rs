use super::*;

/// A system layer that sends everything through one proxy, so a test sees whether the system was
/// consulted at all.
struct AlwaysProxy;

impl SystemProxies for AlwaysProxy {
    fn route(&self, _url: &Url) -> Route {
        Route::Proxy("http://system.proxy:3128".into())
    }
}

fn env(vars: &[(&str, &str)]) -> EnvProxies {
    EnvProxies::from_vars(|name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()))
}

fn decide_for(url: &str, env: &EnvProxies) -> Route {
    decide(&Url::parse(url).expect("a valid test URL"), env, &AlwaysProxy)
}

#[test]
fn loopback_link_local_and_localhost_always_go_direct() {
    let with_env_proxy = env(&[
        ("HTTP_PROXY", "http://env.proxy:3128"),
        ("HTTPS_PROXY", "http://env.proxy:3128"),
    ]);
    for url in [
        "http://127.0.0.1:8080/health",
        "http://127.8.9.10/",
        "http://localhost:11434/v1",
        "http://LOCALHOST./",
        "http://api.localhost/",
        "http://[::1]:5000/",
        "http://[::ffff:127.0.0.1]/",
        "http://169.254.1.2/",
        "http://[fe80::1]/",
        "http://0.0.0.0:9000/",
    ] {
        assert_eq!(
            decide_for(url, &with_env_proxy),
            Route::Direct,
            "{url} under an env proxy"
        );
        assert_eq!(
            decide_for(url, &EnvProxies::default()),
            Route::Direct,
            "{url} under a system proxy"
        );
    }
}

#[test]
fn lan_and_public_hosts_are_not_local() {
    for url in [
        "http://192.168.1.20/",
        "http://10.0.0.5/",
        "http://nas.local/",
        "https://example.com/",
        "http://[fd00::1]/",
    ] {
        assert_eq!(
            decide_for(url, &EnvProxies::default()),
            Route::Proxy("http://system.proxy:3128".into()),
            "{url}"
        );
    }
}

#[test]
fn the_environment_wins_over_the_system() {
    let env = env(&[("HTTPS_PROXY", "env.proxy:8080")]);
    assert_eq!(
        decide_for("https://example.com/", &env),
        Route::Proxy("http://env.proxy:8080".into())
    );
}

#[test]
fn a_scheme_the_environment_leaves_unset_falls_to_the_system() {
    let env = env(&[("HTTPS_PROXY", "http://env.proxy:8080")]);
    assert_eq!(
        decide_for("http://example.com/", &env),
        Route::Proxy("http://system.proxy:3128".into())
    );
}

#[test]
fn no_proxy_overrides_the_system_proxy_too() {
    let env = env(&[("NO_PROXY", "example.com")]);
    assert_eq!(decide_for("https://example.com/", &env), Route::Direct);
}
