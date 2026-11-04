//! A web image in a Markdown document, fetched by the app (`markdown_image_fetch.rs`):
//! only after the user allows it, once per image, and only when it's an image.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::markdown_image_fetch::RemoteImage;
use super::media::DeferredMedia as _;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n and the rest";

/// A web server on its own runtime, so the test thread can block in `fetch` the way
/// the scheme handler's blocking thread does.
fn serving(route: &str, response: ResponseTemplate) -> (tokio::runtime::Runtime, MockServer) {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    });
    (runtime, server)
}

fn image(url: String, allowed: bool) -> (RemoteImage, Arc<AtomicBool>) {
    let allowed = Arc::new(AtomicBool::new(allowed));
    let image = RemoteImage::new(url, Arc::clone(&allowed), Arc::new(AtomicBool::new(false)));
    (image, allowed)
}

#[test]
fn a_web_image_is_fetched_only_after_the_user_allows_it() {
    let (runtime, server) = serving("/shot.png", ResponseTemplate::new(200).set_body_bytes(PNG));
    let (image, allowed) = image(format!("{}/shot.png", server.uri()), false);

    assert!(image.fetch().is_none());
    let asked = runtime.block_on(server.received_requests()).unwrap_or_default();
    assert!(asked.is_empty(), "nothing goes over the network before the click");

    allowed.store(true, Ordering::Relaxed);
    let entry = image.fetch().expect("served once allowed");
    assert_eq!(entry.mime, "image/png");
    assert_eq!(std::fs::read(&entry.canonical_path).unwrap(), PNG);
    image.fetch().expect("a second request reuses the download");
    let asked = runtime.block_on(server.received_requests()).unwrap_or_default();
    assert_eq!(asked.len(), 1, "one download per image");
}

#[test]
fn a_badge_with_no_extension_is_an_svg_by_its_content_type() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#;
    let response = ResponseTemplate::new(200).set_body_raw(svg.to_vec(), "image/svg+xml;charset=utf-8");
    let (_runtime, server) = serving("/badge/build-passing-green", response);
    let (image, _) = image(format!("{}/badge/build-passing-green", server.uri()), true);
    assert_eq!(image.fetch().expect("served").mime, "image/svg+xml");
}

#[test]
fn a_web_page_that_isnt_an_image_is_refused() {
    let response = ResponseTemplate::new(200).set_body_raw(b"<html>hi</html>".to_vec(), "text/html");
    let (_runtime, server) = serving("/fake.png", response);
    let (image, _) = image(format!("{}/fake.png", server.uri()), true);
    assert!(image.fetch().is_none());
}

#[test]
fn a_failed_request_serves_nothing() {
    let (_runtime, server) = serving("/gone.png", ResponseTemplate::new(404));
    let (image, _) = image(format!("{}/gone.png", server.uri()), true);
    assert!(image.fetch().is_none());
}
