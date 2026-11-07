use super::{classify, image_file_name};
use crate::drop_in::events::DragPayloadKind;

fn types(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

/// `NSFilePromiseReceiver.readableDraggedTypes` on macOS 27.0.1 (probe, 2026-11-07).
fn promise_types() -> Vec<String> {
    types(&[
        "com.apple.NSFilePromiseItemMetaData",
        "dyn.ah62d4rv4gu8yc6durvwwa3xmrvw1gkdusm1044pxqyuha2pxsvw0e55bsmwca7d3sbwu",
        "com.apple.pasteboard.promised-file-content-type",
    ])
}

#[test]
fn a_chrome_image_drag_is_a_promise_though_it_also_carries_bytes() {
    // The shape `web_drag_source_mac.mm` writes for an image: a Carbon-style promise
    // (which AppKit also derives the legacy types from), the bytes, and the URL.
    let chrome = types(&[
        "org.chromium.chromium-initiated-drag",
        "public.url",
        "com.apple.pasteboard.promised-file-url",
        "com.apple.pasteboard.promised-file-content-type",
        "public.jpeg",
    ]);
    assert_eq!(classify(&chrome, &promise_types(), false), DragPayloadKind::Promises);
}

#[test]
fn a_legacy_promise_counts_through_its_derived_type() {
    let mail = types(&[
        "Apple Web Archive pasteboard type",
        "dyn.ah62d4rv4gu8yc6durvwwa3xmrvw1gkdusm1044pxqyuha2pxsvw0e55bsmwca7d3sbwu",
    ]);
    assert_eq!(classify(&mail, &promise_types(), false), DragPayloadKind::Promises);
}

#[test]
fn file_urls_win_over_everything_else() {
    let finder = types(&["public.file-url", "NSFilenamesPboardType", "public.png"]);
    assert_eq!(classify(&finder, &promise_types(), false), DragPayloadKind::FileUrls);
    let legacy = types(&["NSFilenamesPboardType", "com.apple.NSFilePromiseItemMetaData"]);
    assert_eq!(classify(&legacy, &promise_types(), false), DragPayloadKind::FileUrls);
}

#[test]
fn bare_image_bytes_are_image_data() {
    let image = types(&["public.tiff", "public.utf8-plain-text"]);
    assert_eq!(classify(&image, &promise_types(), false), DragPayloadKind::ImageData);
}

#[test]
fn text_and_links_carry_nothing_droppable() {
    let text = types(&["public.utf8-plain-text", "public.url", "public.html"]);
    assert_eq!(classify(&text, &promise_types(), false), DragPayloadKind::Nothing);
    assert_eq!(classify(&[], &promise_types(), false), DragPayloadKind::Nothing);
}

#[test]
fn an_in_app_drag_is_never_read_as_a_promise() {
    // A virtual pane's drag carries ONLY promises; receiving them back into Cmdr would
    // round-trip the files through a temp folder.
    let virtual_session = types(&["com.apple.NSFilePromiseItemMetaData"]);
    assert_eq!(
        classify(&virtual_session, &promise_types(), true),
        DragPayloadKind::InApp
    );
}

#[test]
fn an_image_is_named_after_its_url_with_the_real_extension() {
    assert_eq!(
        image_file_name(Some("https://example.com/pics/cat.webp?w=200"), "png"),
        "cat.png"
    );
    assert_eq!(
        image_file_name(Some("https://example.com/a/sunset"), "jpg"),
        "sunset.jpg"
    );
    assert_eq!(
        image_file_name(Some("https://example.com/my%20dog.jpeg"), "jpg"),
        "my dog.jpg"
    );
}

#[test]
fn an_unusable_url_falls_back_to_a_plain_name() {
    assert_eq!(image_file_name(None, "png"), "image.png");
    assert_eq!(image_file_name(Some("https://example.com/"), "png"), "image.png");
    assert_eq!(image_file_name(Some("not a url"), "png"), "image.png");
    assert_eq!(
        image_file_name(Some("https://example.com/a%2Fb.png"), "png"),
        "image.png"
    );
    assert_eq!(image_file_name(Some("https://example.com/..."), "png"), "image.png");
}
