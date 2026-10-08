use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

const SCRIPT: &str = r#"function FindProxyForURL(url, host) {
  if (host == "example.com") return "PROXY proxy.test:3128; DIRECT";
  return "DIRECT";
}"#;

/// Serves `body` as a PAC file on `127.0.0.1` and counts the requests. A `None` body accepts
/// connections and never answers, like a PAC host that hangs.
fn pac_server(body: Option<&'static str>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let address = listener.local_addr().expect("a bound address");
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            match body {
                Some(body) => {
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(reply.as_bytes());
                }
                None => held.push(stream),
            }
        }
    });
    (format!("http://{address}/proxy.pac"), hits)
}

fn url(s: &str) -> Url {
    Url::parse(s).expect("a valid test URL")
}

fn via_proxy() -> Vec<Entry> {
    vec![
        Entry::Http {
            host: "proxy.test".into(),
            port: 3128,
            credentials: None,
        },
        Entry::Direct,
    ]
}

#[test]
fn a_pac_file_routes_each_host_its_own_way() {
    let (pac, _) = pac_server(Some(SCRIPT));
    let evaluator = MacPac::new(TIMEOUT);
    let source = PacSource::Url(pac);
    assert_eq!(
        evaluator.evaluate(&source, &url("https://example.com")),
        Some(via_proxy())
    );
    assert_eq!(
        evaluator.evaluate(&source, &url("https://other.test")),
        Some(vec![Entry::Direct])
    );
}

#[test]
fn an_inline_pac_script_runs_too() {
    let evaluator = MacPac::new(TIMEOUT);
    let source = PacSource::Script(SCRIPT.into());
    assert_eq!(
        evaluator.evaluate(&source, &url("http://example.com")),
        Some(via_proxy())
    );
}

/// What CFNetwork makes of a PAC that answers `answer` for every URL.
fn pac_answer(answer: &str) -> Option<Vec<Entry>> {
    let script = format!(r#"function FindProxyForURL(url, host) {{ return "{answer}"; }}"#);
    MacPac::new(TIMEOUT).evaluate(&PacSource::Script(script), &url("https://example.com"))
}

fn socks_test(port: u16) -> Entry {
    Entry::Socks {
        host: "socks.test".into(),
        port,
        credentials: None,
    }
}

#[test]
fn a_socks_answer_is_a_socks_entry() {
    assert_eq!(
        pac_answer("SOCKS socks.test:1080; DIRECT"),
        Some(vec![socks_test(1080), Entry::Direct])
    );
    assert_eq!(pac_answer("socks socks.test:1081"), Some(vec![socks_test(1081)]));
}

#[test]
fn cfnetwork_drops_the_versioned_socks_keywords() {
    // CFNetwork's PAC parser knows `SOCKS` alone; `SOCKS5` and `SOCKS4` (Chrome and Firefox
    // extensions) vanish from the list, so they're as invisible to Cmdr as to Safari (verified on
    // macOS 27.0, 2026-10-08). A regression anchor: if this starts failing, macOS learned them.
    assert_eq!(pac_answer("SOCKS5 socks.test:1080; DIRECT"), Some(vec![Entry::Direct]));
    assert_eq!(pac_answer("SOCKS4 socks.test:1080; DIRECT"), Some(vec![Entry::Direct]));
    assert_eq!(pac_answer("SOCKS5 socks.test:1080"), Some(vec![]));
}

#[test]
fn an_answer_is_cached_per_host() {
    let (pac, hits) = pac_server(Some(SCRIPT));
    let evaluator = MacPac::new(TIMEOUT);
    let source = PacSource::Url(pac);
    assert_eq!(
        evaluator.evaluate(&source, &url("https://example.com")),
        Some(via_proxy())
    );
    // One evaluation can take CFNetwork more than one fetch (three, on macOS 27.0), so the cache is
    // judged by what the repeats add: nothing.
    let after_first = hits.load(Ordering::SeqCst);
    for _ in 0..2 {
        assert_eq!(
            evaluator.evaluate(&source, &url("https://example.com")),
            Some(via_proxy())
        );
    }
    assert_eq!(hits.load(Ordering::SeqCst), after_first);
}

#[test]
fn an_unreachable_pac_fails_without_waiting_out_the_timeout() {
    let closed = TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .expect("an address");
    let evaluator = MacPac::new(TIMEOUT);
    let started = Instant::now();
    let answer = evaluator.evaluate(
        &PacSource::Url(format!("http://{closed}/proxy.pac")),
        &url("https://example.com"),
    );
    assert_eq!(answer, None);
    assert!(started.elapsed() < TIMEOUT, "took {:?}", started.elapsed());
}

#[test]
fn a_pac_host_that_never_answers_is_given_up_on() {
    let (pac, _) = pac_server(None);
    let evaluator = MacPac::new(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(
        evaluator.evaluate(&PacSource::Url(pac), &url("https://example.com")),
        None
    );
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(300) && took < Duration::from_secs(2),
        "took {took:?}"
    );
}

#[test]
fn a_broken_script_is_a_failure() {
    let evaluator = MacPac::new(TIMEOUT);
    let answer = evaluator.evaluate(
        &PacSource::Script("this is not javascript (".into()),
        &url("https://example.com"),
    );
    assert_eq!(answer, None);
}
