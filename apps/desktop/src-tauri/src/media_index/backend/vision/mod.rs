//! The real macOS [`VisionBackend`]: OCR via Vision's `VNRecognizeTextRequest`, fed
//! a downscaled, in-memory decode from ImageIO (`CGImageSource`) — no thumbnail
//! files on disk (plan Decision 5).
//!
//! ## Threading + the 8 MB stack
//!
//! Vision's `performRequests` and ImageIO's decode do synchronous XPC round-trips
//! into system daemons (the ANE via `com.apple.vision`), which can overrun a small
//! worker stack — the same hazard as calling AppKit off rayon (`src-tauri/CLAUDE.md`:
//! "Never use rayon for calls into macOS frameworks; use dedicated 8 MB-stack OS
//! threads"). So this backend owns ONE dedicated OS thread with an 8 MB stack, and
//! [`ocr`](VisionOcrBackend::ocr) dispatches each image to it over a channel and
//! blocks for the result. Running on a single thread also SERIALIZES Vision calls,
//! which Apple recommends for pooled inference (the CLIP spike's note).
//!
//! Every Vision/ImageIO call runs inside an [`objc2::rc::autoreleasepool`] on that
//! thread, so the framework's autoreleased temporaries (the decoded image, the
//! observations) are freed per image rather than piling up across the whole pass.
//!
//! ## FFI discipline
//!
//! Every `unsafe` block below carries a per-site `// SAFETY:` naming the concrete
//! invariant (pointer validity, Create-vs-Get ownership, success-gate), never a
//! blanket allow (`src-tauri/CLAUDE.md`). The Vision/ImageIO objects are created and
//! dropped entirely within the worker thread, so none of them cross a thread
//! boundary; only the input path (`String`) and the result (`OcrResult`) do.
//!
//! ## Hostile input
//!
//! A broken, empty, non-decodable, or unreadable file returns a typed
//! [`VisionError`] (never a panic or a hang): the read, the `CGImageSource` create,
//! and the thumbnail decode each fail closed to [`VisionError::Decode`], and a Vision
//! request failure to [`VisionError::Ocr`]. The scheduler logs it and marks the row
//! `Failed`.

use std::sync::mpsc;
use std::thread;

use objc2::AnyThread;
use objc2::rc::autoreleasepool;
use objc2_core_foundation::{
    CFData, CFDictionary, CFNumber, CFNumberType, CFRetained, CFString, kCFBooleanTrue, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks,
};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSArray, NSDictionary, NSProcessInfo};
use objc2_image_io::{
    CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceThumbnailMaxPixelSize,
};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};

use super::{ImageInput, OcrResult, VisionBackend, VisionError};

/// The longest-edge pixel size the in-memory decode downscales to before OCR. Vision
/// text recognition gains little above a few thousand pixels while a full-resolution
/// decode of a 48-megapixel photo would spike ~190 MB of bitmap; capping the long
/// edge here bounds the decoded bitmap to ~36 MB and keeps small text legible
/// (plan Decision 5 — feed a downscaled decode, never the original).
const MAX_OCR_DIMENSION: i64 = 3072;

/// One OCR job handed to the worker thread: the image identity, its byte source,
/// and a one-shot reply channel. `bytes` is `Some` when the enrich layer already
/// fetched the compressed image (the network case — read under a timeout off this
/// thread); `None` means read `path` here (the local case).
struct Job {
    path: String,
    bytes: Option<Vec<u8>>,
    respond: mpsc::Sender<Result<OcrResult, VisionError>>,
}

/// The real Vision OCR backend. Holds the OS/Vision engine stamp and the channel to
/// its dedicated 8 MB-stack worker thread. `Send + Sync` (the channel sender is), so
/// an `Arc<dyn VisionBackend>` can be shared by the scheduler.
pub struct VisionOcrBackend {
    engine_version: String,
    sender: mpsc::SyncSender<Job>,
}

impl VisionOcrBackend {
    /// Spawn the dedicated OCR worker thread and compute the engine stamp.
    pub fn new() -> Self {
        let engine_version = compute_engine_version();
        // A small bound: `ocr` sends one job then blocks for its reply, so at most a
        // few are ever queued even under concurrent callers.
        let (sender, receiver) = mpsc::sync_channel::<Job>(8);
        thread::Builder::new()
            .name("media-vision-ocr".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || worker_loop(receiver))
            .expect("spawn media-vision-ocr worker thread");
        Self { engine_version, sender }
    }
}

impl Default for VisionOcrBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VisionBackend for VisionOcrBackend {
    fn engine_version(&self) -> String {
        self.engine_version.clone()
    }

    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Job {
                path: input.path.clone(),
                bytes: input.bytes.clone(),
                respond: tx,
            })
            .map_err(|_| VisionError::Ocr("vision OCR worker thread is gone".to_string()))?;
        rx.recv()
            .map_err(|_| VisionError::Ocr("vision OCR worker dropped the job".to_string()))?
    }
}

/// The worker thread's loop: run each job inside its own autoreleasepool so the
/// framework temporaries are freed per image, and reply on its channel. Exits when
/// the backend (and thus the sender) is dropped.
fn worker_loop(receiver: mpsc::Receiver<Job>) {
    while let Ok(job) = receiver.recv() {
        let result = autoreleasepool(|_| recognize_text(&job.path, job.bytes.as_deref()));
        // The caller may have gone away (a cancelled pass); dropping the reply is fine.
        let _ = job.respond.send(result);
    }
}

/// Compute the engine stamp: the macOS version plus the current Vision OCR request
/// revision. Both bump when the OS ships a new OCR engine, so a stored row's stamp
/// mismatches and re-runs (data-coverage — plan M1). Cheap and stable within an OS
/// version.
fn compute_engine_version() -> String {
    autoreleasepool(|_| {
        let info = NSProcessInfo::processInfo();
        let v = info.operatingSystemVersion();
        // A freshly created request defaults to the current OCR revision for this OS,
        // so its `revision` is the engine marker we want (it bumps when the OS ships a
        // new text recognizer). Read it off an instance rather than the base
        // `VNRequest` class accessor, which would report the wrong subclass's revision.
        let request = VNRecognizeTextRequest::new();
        // SAFETY: `revision` is a plain accessor on a valid, just-created request; it
        // returns the request's revision as an integer.
        let revision = unsafe { request.revision() };
        format!(
            "vision-ocr;os={}.{}.{};rev={}",
            v.majorVersion, v.minorVersion, v.patchVersion, revision
        )
    })
}

/// Decode an image downscaled in-memory and run Vision text recognition over it,
/// returning the recognized lines newline-joined. Fails closed to a typed
/// [`VisionError`] on any hostile input.
///
/// `prefetched` carries the compressed bytes when the enrich layer already read them
/// (the network case — read under a timeout OFF this thread); when `None`, the bytes
/// are read from `path` here (the local case). `path` is used for error messages
/// either way.
///
/// Must run on the dedicated worker thread, inside an autoreleasepool.
fn recognize_text(path: &str, prefetched: Option<&[u8]>) -> Result<OcrResult, VisionError> {
    // Use the pre-fetched bytes when present; otherwise read the compressed bytes
    // here (bounded — a photo/RAW is tens of MB at most). The memory hazard is the
    // DECODED bitmap, which the downscaled thumbnail below caps. A network read is
    // NEVER done here (it would block this serialized OCR thread on a hung mount);
    // the enrich layer fetches network bytes under a timeout and passes them in.
    let owned;
    let bytes: &[u8] = match prefetched {
        Some(b) => b,
        None => {
            owned = std::fs::read(path).map_err(|e| VisionError::Decode(format!("read '{path}': {e}")))?;
            &owned
        }
    };

    // SAFETY: `bytes.as_ptr()` is valid for `bytes.len()` initialized bytes for the
    // duration of the call; `CFDataCreate` copies them, so the buffer needn't outlive
    // it. A null allocator selects the default. Returns a +1 `CFRetained` (Create rule).
    let data = unsafe { CFData::new(None, bytes.as_ptr(), bytes.len() as isize) }
        .ok_or_else(|| VisionError::Decode(format!("CFData allocation failed for '{path}'")))?;

    // SAFETY: `data` is a valid `CFData`; a null options dictionary is allowed.
    // Returns a +1 `CFRetained` (Create rule), or `None` for undecodable data.
    let source = unsafe { CGImageSource::with_data(&data, None) }
        .ok_or_else(|| VisionError::Decode(format!("no image source for '{path}'")))?;

    let options = thumbnail_options();
    // SAFETY: `source` is a valid `CGImageSource`; index 0 is the primary image (a
    // decodable source has count >= 1, and an out-of-range index yields `None`, not
    // UB); `options` is a valid CFDictionary of the documented ImageIO keys. Returns a
    // +1 `CFRetained<CGImage>` (Create rule), or `None` if the image can't be decoded.
    let cg_image = unsafe { source.thumbnail_at_index(0, Some(&options)) }
        .ok_or_else(|| VisionError::Decode(format!("thumbnail decode failed for '{path}'")))?;

    let text = run_recognize(&cg_image).map_err(|e| VisionError::Ocr(format!("'{path}': {e}")))?;
    Ok(OcrResult { text })
}

/// Build the ImageIO thumbnail options: always synthesize from the full image,
/// respect the EXIF orientation (so text is upright), and cap the long edge at
/// [`MAX_OCR_DIMENSION`].
fn thumbnail_options() -> CFRetained<CFDictionary> {
    let max_dim = MAX_OCR_DIMENSION;
    // SAFETY: a valid pointer to `max_dim` (lives for this call); the type tag matches
    // the `i64` payload. Returns a +1 `CFRetained` (Create rule).
    let max_number = unsafe {
        CFNumber::new(
            None,
            CFNumberType::SInt64Type,
            (&raw const max_dim).cast::<core::ffi::c_void>(),
        )
    }
    .expect("CFNumberCreate never returns null for a valid SInt64");

    // Keys are the ImageIO CFString constants; values are the CFNumber and the CF
    // boolean true. The arrays hold `*const c_void` element pointers into objects that
    // outlive the `CFDictionaryCreate` call (which retains them via the CF callbacks).
    // SAFETY: `kCFBooleanTrue` and the three `kCGImageSource*` keys are non-null
    // CoreFoundation/ImageIO constant statics on macOS; reading an extern static is
    // `unsafe`, and these are immutable framework constants valid for the whole run.
    let (mut keys, mut values): ([*const core::ffi::c_void; 3], [*const core::ffi::c_void; 3]) = unsafe {
        let true_value: *const core::ffi::c_void =
            (kCFBooleanTrue.expect("kCFBooleanTrue is a CF constant") as *const CFBooleanRef).cast();
        let keys = [
            (kCGImageSourceThumbnailMaxPixelSize as *const CFString).cast(),
            (kCGImageSourceCreateThumbnailFromImageAlways as *const CFString).cast(),
            (kCGImageSourceCreateThumbnailWithTransform as *const CFString).cast(),
        ];
        let values = [(&*max_number as *const CFNumber).cast(), true_value, true_value];
        (keys, values)
    };

    // SAFETY: `keys`/`values` are valid, equal-length (3) arrays of CF object
    // pointers; the standard CF type key/value callbacks retain+release them (so they
    // needn't outlive this dictionary); a null allocator selects the default. Returns a
    // +1 `CFRetained` (Create rule).
    unsafe {
        CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            3,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        )
    }
    .expect("CFDictionaryCreate never returns null for valid inputs")
}

/// The concrete `CFBoolean` type behind `kCFBooleanTrue`. `objc2-core-foundation`
/// types it as `CFBoolean`; aliased locally only to keep the pointer casts above
/// readable.
type CFBooleanRef = objc2_core_foundation::CFBoolean;

/// Run `VNRecognizeTextRequest` over an already-decoded `CGImage` and return the
/// recognized text, newline-joined across regions (top candidate per region).
fn run_recognize(cg_image: &CGImage) -> Result<String, String> {
    // No per-image options for the handler (orientation was already applied by the
    // ImageIO transform).
    let empty = NSDictionary::<VNImageOption, objc2::runtime::AnyObject>::new();
    // SAFETY: `VNImageRequestHandler::alloc()` yields a fresh unregistered instance;
    // `cg_image` is a valid `CGImage`; `empty` is a valid (empty) options dictionary.
    // `initWithCGImage:options:` consumes the allocation and returns the initialized,
    // retained handler.
    let handler =
        unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), cg_image, &empty) };

    let request = VNRecognizeTextRequest::new();
    // Accurate is the higher-quality recognizer; language correction reduces spurious
    // splits. Both are safe setters.
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);

    // Vision wants an `NSArray<VNRequest>`; a `VNRecognizeTextRequest` reference
    // coerces to its `VNRequest` superclass.
    let request_ref: &VNRequest = &request;
    let requests = NSArray::from_slice(&[request_ref]);

    handler
        .performRequests_error(&requests)
        .map_err(|e| format!("performRequests failed: {e}"))?;

    // `request` was just performed by the handler above; each result element is a
    // `VNRecognizedTextObservation` for a text request.
    let results = request.results();
    let mut lines = Vec::new();
    if let Some(observations) = results {
        for observation in &observations {
            if let Some(text_obs) = observation.downcast_ref::<VNRecognizedTextObservation>() {
                let candidates = text_obs.topCandidates(1);
                if let Some(best) = candidates.iter().next() {
                    lines.push(best.string().to_string());
                }
            }
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests;
