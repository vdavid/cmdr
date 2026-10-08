//! The PDF reads `agent::tools::read::inspect::pdf` makes, under the same panic
//! containment: `pdf-extract` panics on hostile input by design, and the app wraps
//! every call in `crash_reporter::contain_panics` (a `catch_unwind`). So a panic is
//! NOT a finding here; what is: a stack overflow, an abort, a runaway allocation, or
//! a hang, none of which `catch_unwind` can contain. Mirrored because fuzzing the app
//! crate would instrument all of Tauri; keep the calls in step with `read_pdf_with_cap`.
#![no_main]

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use pdf_extract::{Document, Object, ObjectId};

fn contain<T>(f: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// A copy of the app's guard (`inspect/pdf.rs`): `pdf-extract` follows `Parent` with no
/// visited set, so a cycle is a stack overflow. Keep the two in step.
fn parent_chain_ends(doc: &Document, id: ObjectId) -> bool {
    let mut seen = HashSet::new();
    let mut current = id;
    loop {
        if !seen.insert(current) {
            return false;
        }
        let Ok(dict) = doc.get_dictionary(current) else {
            return true;
        };
        match dict.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => current = parent,
            Err(_) => return true,
        }
    }
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    // libFuzzer's hook turns every panic into an abort; the app's crash reporter lets a
    // contained one pass. Silence it the same way, once.
    static QUIET_PANICS: Once = Once::new();
    QUIET_PANICS.call_once(|| std::panic::set_hook(Box::new(|_| {})));

    let Some(Ok(doc)) = contain(|| Document::load_mem(data)) else {
        return;
    };
    let Some(pages) = contain(|| doc.get_pages()) else {
        return;
    };
    if contain(|| doc.is_encrypted()).unwrap_or(true) {
        return;
    }
    for key in [&b"Title"[..], b"Author"] {
        contain(|| {
            let info = doc.trailer.get(b"Info").ok()?;
            let (_, info) = doc.dereference(info).ok()?;
            let value = info.as_dict().ok()?.get(key).ok()?;
            let (_, value) = doc.dereference(value).ok()?;
            pdf_extract::decode_text_string(value).ok()
        });
    }
    // The default window: the first three pages, each behind the app's `parent_chain_ends`.
    for (&page, &id) in pages.iter().take(3) {
        if !contain(|| parent_chain_ends(&doc, id)).unwrap_or(false) {
            continue;
        }
        let mut text = String::new();
        contain(|| {
            let mut output = pdf_extract::PlainTextOutput::new(&mut text);
            let _ = pdf_extract::output_doc_page(&doc, &mut output, page);
        });
    }
});
