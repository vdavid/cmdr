use std::cell::RefCell;

use super::*;

fn http(host: &str, port: u16) -> Entry {
    Entry::Http {
        host: host.to_string(),
        port,
        credentials: None,
    }
}

fn socks(host: &str, port: u16) -> Entry {
    Entry::Socks {
        host: host.to_string(),
        port,
        credentials: None,
    }
}

fn url() -> Url {
    Url::parse("https://example.com").expect("a valid test URL")
}

/// A PAC evaluator with a canned answer that records what it was asked.
struct FakePac {
    answer: Option<Vec<Entry>>,
    asked: RefCell<Vec<PacSource>>,
}

impl FakePac {
    fn answering(answer: Option<Vec<Entry>>) -> Self {
        Self {
            answer,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Pac for FakePac {
    fn evaluate(&self, source: &PacSource, _url: &Url) -> Option<Vec<Entry>> {
        self.asked.borrow_mut().push(source.clone());
        self.answer.clone()
    }
}

fn walk(entries: &[Entry]) -> Route {
    first_route(entries, &url(), &FakePac::answering(None))
}

#[test]
fn the_first_usable_entry_wins() {
    assert_eq!(
        walk(&[http("proxy.test", 3128), Entry::Direct]),
        Route::Proxy("http://proxy.test:3128".into())
    );
    assert_eq!(walk(&[Entry::Direct, http("proxy.test", 3128)]), Route::Direct);
}

#[test]
fn an_entry_cmdr_cant_speak_is_skipped() {
    let entries = [Entry::Unsupported("kCFProxyTypeFTP".into()), http("proxy.test", 8080)];
    assert_eq!(walk(&entries), Route::Proxy("http://proxy.test:8080".into()));
}

#[test]
fn an_empty_or_exhausted_list_means_direct() {
    assert_eq!(walk(&[]), Route::Direct);
    assert_eq!(walk(&[Entry::Unsupported("kCFProxyTypeFTP".into())]), Route::Direct);
}

#[test]
fn a_socks_entry_routes_through_socks5_with_the_proxy_resolving_names() {
    assert_eq!(
        walk(&[socks("socks.test", 1080), Entry::Direct]),
        Route::Proxy("socks5h://socks.test:1080".into())
    );
}

#[test]
fn credentials_and_ipv6_hosts_make_a_valid_proxy_url() {
    let entry = Entry::Http {
        host: "fd00::1".into(),
        port: 3128,
        credentials: Some(("ada".into(), "p@ss word".into())),
    };
    // `%20`, not `+`: hyper-util percent-decodes userinfo, and `+` would reach the proxy as a `+`.
    assert_eq!(
        walk(&[entry]),
        Route::Proxy("http://ada:p%40ss%20word@[fd00::1]:3128".into())
    );
    let entry = Entry::Socks {
        host: "socks.test".into(),
        port: 1080,
        credentials: Some(("ada".into(), "p:ss/word".into())),
    };
    assert_eq!(
        walk(&[entry]),
        Route::Proxy("socks5h://ada:p%3Ass%2Fword@socks.test:1080".into())
    );
}

#[test]
fn a_pac_entry_stands_for_the_pacs_answer() {
    let pac = FakePac::answering(Some(vec![
        Entry::Unsupported("kCFProxyTypeFTP".into()),
        http("pac.proxy", 8080),
    ]));
    let entries = [Entry::AutoConfigUrl("http://wpad/wpad.dat".into()), Entry::Direct];
    assert_eq!(
        first_route(&entries, &url(), &pac),
        Route::Proxy("http://pac.proxy:8080".into())
    );
    assert_eq!(*pac.asked.borrow(), [PacSource::Url("http://wpad/wpad.dat".into())]);
}

#[test]
fn a_pac_that_cant_run_falls_back_to_the_next_entry() {
    let pac = FakePac::answering(None);
    let entries = [Entry::AutoConfigScript("broken".into()), Entry::Direct];
    assert_eq!(first_route(&entries, &url(), &pac), Route::Direct);
    assert_eq!(*pac.asked.borrow(), [PacSource::Script("broken".into())]);
}
