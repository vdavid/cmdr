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
    VNClassifyImageRequest, VNElementType, VNFeaturePrintObservation, VNGenerateImageFeaturePrintRequest,
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};

use super::{Analysis, ImageInput, OcrResult, Tag, VisionBackend, VisionError};

/// The longest-edge pixel size the in-memory decode downscales to before OCR. Vision
/// text recognition gains little above a few thousand pixels while a full-resolution
/// decode of a 48-megapixel photo would spike ~190 MB of bitmap; capping the long
/// edge here bounds the decoded bitmap to ~36 MB and keeps small text legible
/// (plan Decision 5 — feed a downscaled decode, never the original).
const MAX_OCR_DIMENSION: i64 = 3072;

/// Vision refuses an image with ANY pixel dimension under this (it errors "The image is
/// too small in at least one dimension, W x H"). Such files — framework decorations,
/// spacer GIFs, `evd_blackline.tif`-style rules — are indexable-file noise, not photos,
/// so we skip them QUIETLY (a done, empty row) rather than logging a WARN per file per
/// pass. The check is by DIMENSION (typed), never a string-match on the Vision message.
const MIN_ANALYZE_DIMENSION: usize = 3;

/// The most scene/object tags to keep per image, highest-confidence first. Vision's
/// classifier returns the full ~1,300-label taxonomy every time with a confidence per
/// label; keeping only the top few above [`MIN_TAG_SCORE`] holds `media_tags` small
/// and the tags meaningful.
const MAX_TAGS: usize = 12;

/// The minimum classifier confidence a tag must clear to be stored. Most of the
/// taxonomy comes back at near-zero for any given image; this drops the long tail.
const MIN_TAG_SCORE: f32 = 0.1;

/// What the worker should compute for one image. OCR-only serves the focused OCR
/// tests; `Analyze` runs OCR + classify + feature-print on one decode (the enrichment
/// path).
enum JobReply {
    /// Reply with OCR only.
    Ocr(mpsc::Sender<Result<OcrResult, VisionError>>),
    /// Reply with the full analysis (OCR + tags + feature print).
    Analyze(mpsc::Sender<Result<Analysis, VisionError>>),
}

/// One job handed to the worker thread: the image identity, its byte source, and the
/// typed reply channel. `bytes` is `Some` when the enrich layer already fetched the
/// compressed image (the network case — read under a timeout off this thread); `None`
/// means read `path` here (the local case).
struct Job {
    path: String,
    bytes: Option<Vec<u8>>,
    reply: JobReply,
}

/// The real Vision backend. Holds the OCR engine + tag taxonomy stamps, the combined
/// analyze provenance stamp, and the channel to its dedicated 8 MB-stack worker
/// thread. `Send + Sync` (the channel sender is), so an `Arc<dyn VisionBackend>` can be
/// shared by the scheduler.
pub struct VisionOcrBackend {
    engine_version: String,
    taxonomy_version: String,
    analysis_stamp: String,
    sender: mpsc::SyncSender<Job>,
}

impl VisionOcrBackend {
    /// Spawn the dedicated worker thread and compute the provenance stamps.
    pub fn new() -> Self {
        let (engine_version, taxonomy_version, analysis_stamp) = compute_stamps();
        // A small bound: a caller sends one job then blocks for its reply, so at most a
        // few are ever queued even under concurrent callers.
        let (sender, receiver) = mpsc::sync_channel::<Job>(8);
        thread::Builder::new()
            .name("media-vision".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || worker_loop(receiver))
            .expect("spawn media-vision worker thread");
        Self {
            engine_version,
            taxonomy_version,
            analysis_stamp,
            sender,
        }
    }

    /// Send a job to the worker and block for its reply, mapping a dead worker to a
    /// typed error.
    fn dispatch<T>(&self, path: &str, bytes: Option<Vec<u8>>, make_reply: impl FnOnce(mpsc::Sender<T>) -> JobReply) -> T
    where
        T: FromWorkerGone,
    {
        let (tx, rx) = mpsc::channel();
        if self
            .sender
            .send(Job {
                path: path.to_string(),
                bytes,
                reply: make_reply(tx),
            })
            .is_err()
        {
            return T::worker_gone("vision worker thread is gone");
        }
        rx.recv()
            .unwrap_or_else(|_| T::worker_gone("vision worker dropped the job"))
    }
}

/// A reply type that can synthesize its own "worker thread is gone" error, so
/// [`VisionOcrBackend::dispatch`] stays generic over OCR vs full analysis.
trait FromWorkerGone {
    fn worker_gone(msg: &str) -> Self;
}
impl FromWorkerGone for Result<OcrResult, VisionError> {
    fn worker_gone(msg: &str) -> Self {
        Err(VisionError::Ocr(msg.to_string()))
    }
}
impl FromWorkerGone for Result<Analysis, VisionError> {
    fn worker_gone(msg: &str) -> Self {
        Err(VisionError::Ocr(msg.to_string()))
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

    fn taxonomy_version(&self) -> String {
        self.taxonomy_version.clone()
    }

    fn analysis_stamp(&self) -> String {
        self.analysis_stamp.clone()
    }

    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError> {
        self.dispatch(&input.path, input.bytes.clone(), JobReply::Ocr)
    }

    fn analyze(&self, input: &ImageInput) -> Result<Analysis, VisionError> {
        self.dispatch(&input.path, input.bytes.clone(), JobReply::Analyze)
    }
}

/// The worker thread's loop: run each job inside its own autoreleasepool so the
/// framework temporaries are freed per image, and reply on its channel. Exits when
/// the backend (and thus the sender) is dropped.
fn worker_loop(receiver: mpsc::Receiver<Job>) {
    while let Ok(job) = receiver.recv() {
        // The caller may have gone away (a cancelled pass); dropping the reply is fine.
        match job.reply {
            JobReply::Ocr(respond) => {
                let result = autoreleasepool(|_| recognize_text(&job.path, job.bytes.as_deref()));
                let _ = respond.send(result);
            }
            JobReply::Analyze(respond) => {
                let result = autoreleasepool(|_| analyze_image(&job.path, job.bytes.as_deref()));
                let _ = respond.send(result);
            }
        }
    }
}

/// Compute the provenance stamps: the OCR engine stamp, the tag-taxonomy stamp, and
/// the combined analyze stamp. Each carries the macOS version plus the relevant Vision
/// request revision, so any OS upgrade that bumps a recognizer, the tag taxonomy, or
/// the feature-print model mismatches a stored row and re-runs analysis (data-coverage
/// — plan Decision 4). Cheap and stable within an OS version.
///
/// Returns `(engine_version, taxonomy_version, analysis_stamp)`.
fn compute_stamps() -> (String, String, String) {
    autoreleasepool(|_| {
        let info = NSProcessInfo::processInfo();
        let v = info.operatingSystemVersion();
        let os = format!("{}.{}.{}", v.majorVersion, v.minorVersion, v.patchVersion);

        // A freshly created request defaults to the current revision for this OS, so
        // its `revision` is the engine/taxonomy marker (it bumps when the OS ships a
        // new model). Read it off an instance, not the base `VNRequest` class accessor.
        let text_request = VNRecognizeTextRequest::new();
        // SAFETY: `revision` is a plain accessor on a valid, just-created request.
        let ocr_rev = unsafe { text_request.revision() };
        // SAFETY: `new()` constructs a valid classify request; `revision` is a plain
        // accessor on it. Its revision tracks the scene/object tag taxonomy version.
        let classify_rev = unsafe {
            let r = VNClassifyImageRequest::new();
            r.revision()
        };
        // SAFETY: `new()` constructs a valid feature-print request; `revision` is a
        // plain accessor on it. Its revision tracks the feature-print model version.
        let fp_rev = unsafe {
            let r = VNGenerateImageFeaturePrintRequest::new();
            r.revision()
        };

        let engine_version = format!("vision-ocr;os={os};rev={ocr_rev}");
        let taxonomy_version = format!("vision-tax;os={os};rev={classify_rev}");
        let analysis_stamp = format!("{engine_version};tax=rev{classify_rev};fp=rev{fp_rev}");
        (engine_version, taxonomy_version, analysis_stamp)
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
    let cg_image = decode_thumbnail(path, prefetched)?;
    let text = run_recognize(&cg_image).map_err(|e| VisionError::Ocr(format!("'{path}': {e}")))?;
    Ok(OcrResult { text })
}

/// Run the full enrichment analysis — OCR, scene/object tags, and the feature-print
/// embedding — over ONE decode of the image (plan Decision 5), performing all three
/// Vision requests on a single image request handler. Fails closed to a typed
/// [`VisionError`] on hostile input, exactly as [`recognize_text`] does.
///
/// Must run on the dedicated worker thread, inside an autoreleasepool.
fn analyze_image(path: &str, prefetched: Option<&[u8]>) -> Result<Analysis, VisionError> {
    let cg_image = decode_thumbnail(path, prefetched)?;

    // Skip a too-small image QUIETLY (plan M3): Vision would refuse it ("too small in at
    // least one dimension"), so without this it'd log a WARN per file per pass. Detect it
    // by the decoded dimensions (typed, never a string-match on the Vision message) and
    // return an EMPTY analysis — a normal done row that never re-tries (unchanged
    // `(mtime, size)`) and never surfaces in search.
    let image_ref: &CGImage = &cg_image;
    let (width, height) = (CGImage::width(Some(image_ref)), CGImage::height(Some(image_ref)));
    if width < MIN_ANALYZE_DIMENSION || height < MIN_ANALYZE_DIMENSION {
        log::debug!(
            target: "media_index",
            "skipping too-small image '{path}' ({width}x{height}); indexable-file noise, not a photo"
        );
        return Ok(Analysis {
            ocr: OcrResult { text: String::new() },
            tags: Vec::new(),
            embedding: None,
        });
    }

    let empty = NSDictionary::<VNImageOption, objc2::runtime::AnyObject>::new();
    // SAFETY: `alloc()` yields a fresh unregistered instance; `cg_image` is a valid
    // `CGImage`; `empty` is a valid (empty) options dictionary. `initWithCGImage:options:`
    // consumes the allocation and returns the initialized, retained handler.
    let handler =
        unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &cg_image, &empty) };

    let text_request = VNRecognizeTextRequest::new();
    text_request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    text_request.setUsesLanguageCorrection(true);
    // SAFETY: `new()` constructs a valid, autoreleased classify / feature-print request.
    let classify_request = unsafe { VNClassifyImageRequest::new() };
    // SAFETY: as above.
    let feature_request = unsafe { VNGenerateImageFeaturePrintRequest::new() };

    // A `VNRecognizeTextRequest` / `VNClassifyImageRequest` /
    // `VNGenerateImageFeaturePrintRequest` reference each coerce to their `VNRequest`
    // superclass; Vision performs the whole array against the one handler.
    let text_ref: &VNRequest = &text_request;
    let classify_ref: &VNRequest = &classify_request;
    let feature_ref: &VNRequest = &feature_request;
    let requests = NSArray::from_slice(&[text_ref, classify_ref, feature_ref]);

    handler
        .performRequests_error(&requests)
        .map_err(|e| VisionError::Ocr(format!("'{path}': performRequests failed: {e}")))?;

    Ok(Analysis {
        ocr: OcrResult {
            text: read_recognized_text(&text_request),
        },
        tags: read_tags(&classify_request),
        embedding: read_feature_print(&feature_request),
    })
}

/// Decode an image downscaled in-memory (no thumbnail files — plan Decision 5),
/// returning the `CGImage` for the Vision requests. Fails closed to a typed
/// [`VisionError`] on any hostile input.
///
/// `prefetched` carries the compressed bytes when the enrich layer already read them
/// (the network case — read under a timeout OFF this thread); when `None`, the bytes
/// are read from `path` here (the local case). A network read is NEVER done here (it
/// would block this serialized thread on a hung mount).
fn decode_thumbnail(path: &str, prefetched: Option<&[u8]>) -> Result<CFRetained<CGImage>, VisionError> {
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
    unsafe { source.thumbnail_at_index(0, Some(&options)) }
        .ok_or_else(|| VisionError::Decode(format!("thumbnail decode failed for '{path}'")))
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

    Ok(read_recognized_text(&request))
}

/// Read the recognized text off an already-performed [`VNRecognizeTextRequest`], the
/// top candidate per region, newline-joined. Shared by the OCR-only path and the full
/// analysis (which performs the request as part of its batch).
fn read_recognized_text(request: &VNRecognizeTextRequest) -> String {
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
    lines.join("\n")
}

/// Read the scene/object tags off an already-performed [`VNClassifyImageRequest`]:
/// the top [`MAX_TAGS`] classifications above [`MIN_TAG_SCORE`], highest confidence
/// first (Vision already returns them sorted by confidence). An empty/`None` result
/// (no confident tags) yields an empty vec.
fn read_tags(request: &VNClassifyImageRequest) -> Vec<Tag> {
    // SAFETY: `results` is a plain accessor on a just-performed classify request; it
    // returns the classifications (or `None` if none were produced).
    let Some(observations) = (unsafe { request.results() }) else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for obs in &observations {
        // SAFETY: `confidence` is a plain accessor on a valid classification
        // observation from this request.
        let score = unsafe { obs.confidence() };
        if score < MIN_TAG_SCORE {
            // Sorted by confidence: once below the floor, the rest are too.
            break;
        }
        // SAFETY: `identifier` is a plain accessor on the same valid observation; it
        // returns the classification label as a retained `NSString`.
        let label = unsafe { obs.identifier() }.to_string();
        tags.push(Tag { label, score });
        if tags.len() >= MAX_TAGS {
            break;
        }
    }
    tags
}

/// Read the feature-print embedding off an already-performed
/// [`VNGenerateImageFeaturePrintRequest`] as an `f32` vector, or `None` when no
/// observation was produced. The observation stores its raw bytes as either `Float`
/// (4 bytes) or `Double` (8 bytes) elements; both are normalized to `f32`.
fn read_feature_print(request: &VNGenerateImageFeaturePrintRequest) -> Option<Vec<f32>> {
    // SAFETY: `results` is a plain accessor on a just-performed feature-print request.
    let observations = unsafe { request.results() }?;
    let first: objc2::rc::Retained<VNFeaturePrintObservation> = observations.iter().next()?;
    // SAFETY: `elementType`, `elementCount`, and `data` are plain accessors on a valid
    // observation from this request. `data()` returns a +1-retained `NSData`.
    let (element_type, element_count, data) = unsafe { (first.elementType(), first.elementCount(), first.data()) };
    let bytes = data.to_vec();

    let vector: Vec<f32> = match element_type {
        VNElementType::Float => bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        VNElementType::Double => bytes
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]) as f32)
            .collect(),
        // An unknown element type (should never happen) ⇒ no usable embedding.
        _ => return None,
    };

    // Guard against a length/type mismatch (a corrupt observation): the decoded count
    // must match what the observation reported, else drop it rather than store garbage.
    if vector.len() != element_count || vector.is_empty() {
        return None;
    }
    Some(vector)
}

#[cfg(test)]
mod tests;
