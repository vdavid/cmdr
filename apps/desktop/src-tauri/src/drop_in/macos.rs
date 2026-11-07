//! The AppKit side of a drop from another app: reading the dragging pasteboard on
//! enter, and receiving file promises (or writing bare image bytes) on drop.
//!
//! Called from the swizzles in `drag_image_detection.rs`, on the main thread, inside
//! their `catch_unwind`. Every AppKit call that can RAISE goes through
//! `objc2::exception::catch`: an Objective-C exception that reaches Rust aborts the app
//! (CRASH-K35S2). `receivePromisedFilesAtDestination:…` raises when called outside
//! `prepare`/`perform`/`concludeDragOperation:` (verified on macOS 27.0.1 with a
//! standalone probe, 2026-11-07), so it's only ever called from the perform swizzle.

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{ClassType, msg_send};
use objc2_app_kit::{NSFilePromiseReceiver, NSPasteboard, NSRunningApplication, NSView, NSWorkspace};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSOperationQueue, NSString, NSURL};
use tauri::AppHandle;
use tauri_specta::Event as _;

use super::events::{
    DragPayloadKind, DragPayloadKindEvent, DropSource, PromiseDropFailed, PromiseDropFailure, PromiseDropReady,
    PromiseDropStarted,
};
use super::folders::create_drop_folder;
use super::payload::{IMAGE_TYPES, classify, image_file_name};
use super::tally::{DropOutcome, ReceiveTally};
use crate::clipboard::{ClipboardData, ClipboardPayload, pick_clipboard_payload};
use crate::ignore_poison::IgnorePoison;

/// How long a source app gets to deliver a promised drop before the drop gives up.
/// Generous: Chrome may be downloading a big image over a slow link meanwhile.
const RECEIVE_WINDOW: Duration = Duration::from_secs(10 * 60);

/// The source app captured on drag enter, read back on drop.
static ENTERED_APP: Mutex<Option<String>> = Mutex::new(None);

/// Adds the file-promise types to the webview's registered drag types, so a drag that
/// carries ONLY a promise (a Mail attachment) still reaches `draggingEntered:`.
/// WKWebView registers its own set (images, URLs, web archives, files), which already
/// covers a browser image; this closes the promise-only gap.
///
/// # Safety
/// `view` must be the live webview `NSView`, on the main thread.
pub(crate) unsafe fn register_promise_types(view: &AnyObject) {
    // SAFETY: the caller hands the webview object, which is an `NSView` subclass.
    let view: &NSView = unsafe { &*(view as *const AnyObject).cast::<NSView>() };
    let registered = view.registeredDraggedTypes();
    let mut types: Vec<Retained<NSString>> = registered.to_vec();
    for t in NSFilePromiseReceiver::readableDraggedTypes().to_vec() {
        if !types.iter().any(|have| **have == *t) {
            types.push(t);
        }
    }
    let array = NSArray::from_retained_slice(&types);
    if objc2::exception::catch(AssertUnwindSafe(|| view.registerForDraggedTypes(&array))).is_err() {
        log::warn!(target: "drop_in", "couldn't register the file-promise drag types; promise-only drags won't reach Cmdr");
    }
}

/// The dragging pasteboard of `drag_info`.
///
/// # Safety
/// `drag_info` must be the live `id<NSDraggingInfo>` AppKit passed the swizzle.
unsafe fn dragging_pasteboard(drag_info: &AnyObject) -> Option<Retained<NSPasteboard>> {
    // SAFETY: `-draggingPasteboard` is an `NSDraggingInfo` method returning the
    // (non-null, autoreleased) `NSPasteboard`; objc2 retains it for the `Retained`.
    unsafe { msg_send![drag_info, draggingPasteboard] }
}

/// Whether the drag started inside this app: `-draggingSource` is non-nil only then.
///
/// # Safety
/// As [`dragging_pasteboard`].
unsafe fn is_in_app(drag_info: &AnyObject) -> bool {
    // SAFETY: `-draggingSource` is an `NSDraggingInfo` method returning a borrowed `id`
    // (nil for a drag from another app); only its nullness is read.
    let source: *const AnyObject = unsafe { msg_send![drag_info, draggingSource] };
    !source.is_null()
}

/// What `drag_info` carries.
///
/// # Safety
/// As [`dragging_pasteboard`].
unsafe fn payload_kind(drag_info: &AnyObject) -> DragPayloadKind {
    // SAFETY: the caller's contract.
    if unsafe { is_in_app(drag_info) } {
        return DragPayloadKind::InApp;
    }
    // SAFETY: the caller's contract.
    let Some(pasteboard) = (unsafe { dragging_pasteboard(drag_info) }) else {
        return DragPayloadKind::Nothing;
    };
    let types: Vec<String> = pasteboard
        .types()
        .map(|t| t.to_vec().iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let promise_types: Vec<String> = NSFilePromiseReceiver::readableDraggedTypes()
        .to_vec()
        .iter()
        .map(|s| s.to_string())
        .collect();
    classify(&types, &promise_types, false)
}

/// The frontmost app's name, unless it's Cmdr itself (a drag from a background window
/// of another app leaves Cmdr frontmost, and then we don't know the source).
fn frontmost_app_name() -> Option<String> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    if app.processIdentifier() == NSRunningApplication::currentApplication().processIdentifier() {
        return None;
    }
    app.localizedName().map(|n| n.to_string())
}

/// `draggingEntered:`: tells the frontend what the drag carries, and remembers the
/// source app for the drop.
///
/// # Safety
/// As [`dragging_pasteboard`], on the main thread.
pub(crate) unsafe fn on_dragging_entered(app: &AppHandle, drag_info: &AnyObject) {
    // SAFETY: the caller's contract.
    let kind = unsafe { payload_kind(drag_info) };
    *ENTERED_APP.lock_ignore_poison() = match kind {
        DragPayloadKind::Promises | DragPayloadKind::ImageData => frontmost_app_name(),
        _ => None,
    };
    let _ = DragPayloadKindEvent { kind }.emit(app);
}

/// `performDragOperation:`: for a promise or image drop, receives the files into a
/// fresh drop folder and tells the frontend. Every other drop is wry's alone. Emits
/// `promise-drop-started` synchronously, so it reaches the frontend before wry's drop
/// event; the files land later, off the main thread.
///
/// # Safety
/// As [`dragging_pasteboard`], on the main thread, inside `performDragOperation:`.
pub(crate) unsafe fn on_perform_drag_operation(app: &AppHandle, drag_info: &AnyObject) {
    // SAFETY: the caller's contract.
    let kind = unsafe { payload_kind(drag_info) };
    if !matches!(kind, DragPayloadKind::Promises | DragPayloadKind::ImageData) {
        return;
    }
    // SAFETY: the caller's contract.
    let Some(pasteboard) = (unsafe { dragging_pasteboard(drag_info) }) else {
        return;
    };
    let source = DropSource {
        app_name: ENTERED_APP.lock_ignore_poison().take(),
        page_url: pasteboard
            .stringForType(&NSString::from_str("public.url"))
            .map(|s| s.to_string()),
    };

    let (drop_id, dir) = match create_drop_folder() {
        Ok(created) => created,
        Err(e) => {
            log::warn!(target: "drop_in", "couldn't make a drop folder: {:?}", e.kind());
            emit_outcome(
                app,
                "drop-unavailable",
                DropOutcome::Failed(PromiseDropFailure::NoDropFolder),
            );
            return;
        }
    };

    if kind == DragPayloadKind::Promises && receive_promises(app, &pasteboard, &drop_id, &dir, &source) {
        return;
    }
    // No promise AppKit could read (or it raised): the image bytes, when there are any.
    write_image_bytes(app, &pasteboard, drop_id, dir, source);
}

/// Starts receiving every promise on `pasteboard` into `dir`. Returns false when there
/// was nothing to receive (no receiver objects, or reading them raised).
fn receive_promises(
    app: &AppHandle,
    pasteboard: &NSPasteboard,
    drop_id: &str,
    dir: &std::path::Path,
    source: &DropSource,
) -> bool {
    let receivers = match read_receivers(pasteboard) {
        Ok(receivers) if !receivers.is_empty() => receivers,
        Ok(_) => return false,
        Err(exception) => {
            log::warn!(target: "drop_in", "reading the file promises raised: {exception}");
            return false;
        }
    };
    // One reader callback per promised file; a receiver that names no type still
    // promises one.
    let expected: usize = receivers.iter().map(|r| r.fileTypes().count().max(1)).sum();
    let _ = PromiseDropStarted {
        drop_id: drop_id.to_string(),
        item_count: expected,
        source: source.clone(),
    }
    .emit(app);

    let tally = Arc::new(Mutex::new(ReceiveTally::new(expected)));
    let destination = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&dir.to_string_lossy()), true);
    let queue = NSOperationQueue::new();
    let options = NSDictionary::new();
    for receiver in &receivers {
        let reader = reader_block(app.clone(), drop_id.to_string(), Arc::clone(&tally), queue.clone());
        let started = objc2::exception::catch(AssertUnwindSafe(|| {
            // SAFETY: called from `performDragOperation:` (the only place AppKit allows
            // it); `destination` is a file URL to an existing folder, `options` an empty
            // dictionary, `queue` a live operation queue, and `reader` a block with
            // the documented `(NSURL *, NSError *)` signature.
            unsafe {
                receiver.receivePromisedFilesAtDestination_options_operationQueue_reader(
                    &destination,
                    &options,
                    &queue,
                    &reader,
                );
            }
        }));
        if let Err(exception) = started {
            log::warn!(target: "drop_in", "receiving a file promise raised: {exception:?}");
            // Its files will never call back: count them as failed now.
            for _ in 0..receiver.fileTypes().count().max(1) {
                if let Some(outcome) = tally.lock_ignore_poison().record(None) {
                    emit_outcome(app, drop_id, outcome);
                }
            }
        }
    }
    start_watchdog(app.clone(), drop_id.to_string(), tally);
    true
}

/// `-[NSPasteboard readObjectsForClasses:@[NSFilePromiseReceiver] options:nil]`, with
/// a raise caught.
fn read_receivers(pasteboard: &NSPasteboard) -> Result<Vec<Retained<NSFilePromiseReceiver>>, String> {
    objc2::exception::catch(AssertUnwindSafe(|| {
        let class: &AnyClass = NSFilePromiseReceiver::class();
        // SAFETY: `+[NSArray arrayWithObject:]` takes one `id` (a class object is one)
        // and returns an autoreleased array, retained here by objc2.
        let classes: Retained<NSArray> = unsafe { msg_send![NSArray::<AnyObject>::class(), arrayWithObject: class] };
        // SAFETY: `readObjectsForClasses:options:` takes an array of classes conforming
        // to `NSPasteboardReading` (NSFilePromiseReceiver does) and a nullable options
        // dictionary; it returns an autoreleased array or nil.
        let objects: Option<Retained<NSArray>> =
            unsafe { msg_send![pasteboard, readObjectsForClasses: &*classes, options: std::ptr::null::<AnyObject>()] };
        objects
            .map(|array| {
                array
                    .to_vec()
                    .into_iter()
                    .filter_map(|o| o.downcast::<NSFilePromiseReceiver>().ok())
                    .collect()
            })
            .unwrap_or_default()
    }))
    .map_err(|exception| format!("{exception:?}"))
}

/// The reader block AppKit calls on `queue`, once per promised file.
fn reader_block(
    app: AppHandle,
    drop_id: String,
    tally: Arc<Mutex<ReceiveTally>>,
    queue: Retained<NSOperationQueue>,
) -> block2::RcBlock<dyn Fn(NonNull<NSURL>, *mut NSError)> {
    block2::RcBlock::new(move |url: NonNull<NSURL>, error: *mut NSError| {
        // Holds the queue for as long as AppKit holds this block.
        let _queue = &queue;
        // A panic must not unwind into AppKit's queue thread.
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let received = if error.is_null() {
                // SAFETY: AppKit passes a live `NSURL` for the duration of the call.
                unsafe { url.as_ref() }.path().map(|p| PathBuf::from(p.to_string()))
            } else {
                // SAFETY: non-null, so a live `NSError` for the duration of the call.
                let error = unsafe { &*error };
                log::warn!(
                    target: "drop_in",
                    "a promised file didn't arrive: domain={} code={}",
                    error.domain(),
                    error.code()
                );
                None
            };
            let received = received.filter(|p| p.exists());
            if let Some(outcome) = tally.lock_ignore_poison().record(received) {
                emit_outcome(&app, &drop_id, outcome);
            }
        }));
    })
}

/// Gives up on a drop the source app never finished delivering.
fn start_watchdog(app: AppHandle, drop_id: String, tally: Arc<Mutex<ReceiveTally>>) {
    let spawned = std::thread::Builder::new().name("drop-watchdog".into()).spawn(move || {
        std::thread::sleep(RECEIVE_WINDOW);
        if let Some(outcome) = tally.lock_ignore_poison().time_out() {
            emit_outcome(&app, &drop_id, outcome);
        }
    });
    if let Err(e) = spawned {
        log::warn!(target: "drop_in", "couldn't start the drop watchdog: {e}");
    }
}

/// The image-bytes fallback: copies the bytes off the pasteboard here (main thread),
/// then decodes and writes them off it (a TIFF→PNG decode can take hundreds of ms).
fn write_image_bytes(app: &AppHandle, pasteboard: &NSPasteboard, drop_id: String, dir: PathBuf, source: DropSource) {
    let read = |uti: &str| pasteboard.dataForType(&NSString::from_str(uti)).map(|d| d.to_vec());
    let [png, tiff, jpeg] = IMAGE_TYPES.map(read);
    if png.is_none() && tiff.is_none() && jpeg.is_none() {
        emit_outcome(app, &drop_id, DropOutcome::Failed(PromiseDropFailure::NothingReceived));
        return;
    }
    let _ = PromiseDropStarted {
        drop_id: drop_id.clone(),
        item_count: 1,
        source: source.clone(),
    }
    .emit(app);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("drop-image-write".into())
        .spawn(move || {
            let data = ClipboardData {
                png,
                tiff,
                jpeg,
                pdf: None,
                text: None,
            };
            let (bytes, ext) = match pick_clipboard_payload(data) {
                ClipboardPayload::Png(bytes) => (bytes, "png"),
                ClipboardPayload::Jpeg(bytes) => (bytes, "jpg"),
                _ => {
                    emit_outcome(&app, &drop_id, DropOutcome::Failed(PromiseDropFailure::NothingReceived));
                    return;
                }
            };
            let path = dir.join(image_file_name(source.page_url.as_deref(), ext));
            let outcome = match std::fs::write(&path, bytes) {
                Ok(()) => DropOutcome::Ready {
                    paths: vec![path],
                    failed_count: 0,
                },
                Err(e) => {
                    log::warn!(target: "drop_in", "couldn't write a dropped image: {:?}", e.kind());
                    DropOutcome::Failed(PromiseDropFailure::NothingReceived)
                }
            };
            emit_outcome(&app, &drop_id, outcome);
        });
    if let Err(e) = spawned {
        log::warn!(target: "drop_in", "couldn't start the image write: {e}");
    }
}

fn emit_outcome(app: &AppHandle, drop_id: &str, outcome: DropOutcome) {
    match outcome {
        DropOutcome::Ready { paths, failed_count } => {
            let _ = PromiseDropReady {
                drop_id: drop_id.to_string(),
                paths: paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
                failed_count,
            }
            .emit(app);
        }
        DropOutcome::Failed(reason) => {
            log::info!(target: "drop_in", "drop {drop_id} produced no file: {reason:?}");
            // Nothing in it for anyone to copy: let it go now.
            // allowed-discarded-outcome: an id that names no folder (no folder was made) is the expected no-op.
            super::folders::release_drop(drop_id);
            let _ = PromiseDropFailed {
                drop_id: drop_id.to_string(),
                reason,
            }
            .emit(app);
        }
    }
}
