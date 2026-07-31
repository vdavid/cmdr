//! The real macOS Core ML CLIP towers (image + text), behind a dedicated worker thread.
//!
//! Mirrors the Vision backend's threading discipline (`backend/vision.rs`): all Core ML
//! `MLModel` objects are `!Send`, and the synchronous ANE predict is an XPC round-trip
//! that can overrun a small stack, so ONE dedicated 8 MB-stack thread owns both loaded
//! towers and SERIALIZES every predict (Apple's recommendation for pooled inference).
//! `encode_text` (query time) and `encode_image` (enrichment, called from the Vision
//! worker) both send a job to this thread and block for the reply, so no `!Send` object
//! ever crosses a thread boundary — only the input ids / pixel `Vec` in and the embedding
//! `Vec<f32>` out.
//!
//! The `.mlpackage` towers are compiled to `.mlmodelc` on-device at first load (via
//! `compileModelAtURL:error:`) and the compiled bundle is cached beside the model so later
//! launches skip the 1–2 s compile. After a verified compile (both towers load AND encode a
//! sane embedding) the ~550 MB combined `.mlpackage` sources are deleted (plan M5a — the
//! compiled model is all the worker needs); if a compiled model later fails to load (an OS
//! upgrade can invalidate it) with no source to recompile from, [`load_tower`] drops the
//! stale `.mlmodelc` so the feature reads as not-installed and the standard download flow
//! refetches the pinned zip.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_core_ml::{
    MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel, MLModelConfiguration,
    MLMultiArray, MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL};

use super::ClipError;
use super::install::{
    CLIP_TOWERS, ClipTowerSpec, clip_model_dir, compiled_path, drop_compiled, package_path, reclaim_source_package,
};
use super::tokenizer::CONTEXT_LENGTH;

/// The CLIP embedding dimensionality (OpenAI CLIP ViT-B/32). Both towers output this.
const EMBED_DIM: usize = 512;
/// The image tower input side (224×224 RGB, CHW).
const IMAGE_SIDE: usize = 224;
/// The image tower input element count (`1 × 3 × 224 × 224`).
pub const IMAGE_PIXELS: usize = 3 * IMAGE_SIDE * IMAGE_SIDE;

/// A job for the CLIP worker thread. Each carries a reply channel.
enum ClipJob {
    EncodeText {
        ids: Vec<i32>,
        reply: mpsc::Sender<Result<Vec<f32>, ClipError>>,
    },
    EncodeImage {
        pixels: Vec<f32>,
        reply: mpsc::Sender<Result<Vec<f32>, ClipError>>,
    },
}

/// A handle to the CLIP worker thread. Cloneable senders; one thread + two loaded towers.
pub struct ClipWorker {
    sender: mpsc::Sender<ClipJob>,
}

impl ClipWorker {
    /// Spawn the worker thread, lazily loading both towers from `model_dir` on first use.
    /// The thread lives for the process (the global [`worker`]).
    fn spawn(model_dir: PathBuf) -> ClipWorker {
        let (sender, receiver) = mpsc::channel::<ClipJob>();
        thread::Builder::new()
            .name("clip-worker".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || worker_loop(&model_dir, &receiver))
            .expect("spawn clip worker thread");
        ClipWorker { sender }
    }

    /// Encode already-tokenized text ids (length [`CONTEXT_LENGTH`]) to a text embedding.
    pub fn encode_text(&self, ids: Vec<i32>) -> Result<Vec<f32>, ClipError> {
        let (reply, rx) = mpsc::channel();
        self.sender
            .send(ClipJob::EncodeText { ids, reply })
            .map_err(|_| ClipError::NotAvailable)?;
        rx.recv().map_err(|_| ClipError::NotAvailable)?
    }

    /// Encode a CHW `[0,1]` pixel buffer (length [`IMAGE_PIXELS`]) to an image embedding.
    pub fn encode_image(&self, pixels: Vec<f32>) -> Result<Vec<f32>, ClipError> {
        let (reply, rx) = mpsc::channel();
        self.sender
            .send(ClipJob::EncodeImage { pixels, reply })
            .map_err(|_| ClipError::NotAvailable)?;
        rx.recv().map_err(|_| ClipError::NotAvailable)?
    }
}

/// The worker thread: load both towers once (lazily), then serve predict jobs. If a tower
/// fails to load, every job returns the load error (the feature stays gated off).
fn worker_loop(model_dir: &Path, receiver: &mpsc::Receiver<ClipJob>) {
    let models = load_towers(model_dir);
    // M5a: once both towers load AND a zero-input encode is sane (512-d, finite — not the
    // NaN a bad palettization would emit), the ~550 MB combined `.mlpackage` sources are dead
    // weight (the compiled `.mlmodelc` is all the worker needs), so reclaim them. A failed
    // sanity check keeps the sources so a recompile stays possible.
    if let Ok(m) = &models
        && verify_sane(m)
    {
        for tower in CLIP_TOWERS {
            match reclaim_source_package(model_dir, tower) {
                Ok(true) => {
                    log::info!(target: "media_index", "reclaimed CLIP source package '{}' after verified compile", tower.artifact)
                }
                Ok(false) => {}
                Err(e) => log::warn!(target: "media_index", "reclaim of CLIP source '{}' failed: {e}", tower.artifact),
            }
        }
    }
    while let Ok(job) = receiver.recv() {
        match job {
            ClipJob::EncodeText { ids, reply } => {
                let out = match &models {
                    Ok(m) => m.encode_text(&ids),
                    Err(e) => Err(e.clone()),
                };
                let _ = reply.send(out);
            }
            ClipJob::EncodeImage { pixels, reply } => {
                let out = match &models {
                    Ok(m) => m.encode_image(&pixels),
                    Err(e) => Err(e.clone()),
                };
                let _ = reply.send(out);
            }
        }
    }
}

/// Both loaded towers, confined to the worker thread (`MLModel` is `!Send`).
struct ClipModels {
    image: Retained<MLModel>,
    text: Retained<MLModel>,
}

/// Load both towers from the install dir, each with the M5a recovery path.
fn load_towers(model_dir: &Path) -> Result<ClipModels, ClipError> {
    Ok(ClipModels {
        image: load_tower(model_dir, &CLIP_TOWERS[0])?,
        text: load_tower(model_dir, &CLIP_TOWERS[1])?,
    })
}

/// Load one tower, recovering across the M5a package-reclaim (plan M5a):
///
/// 1. Prefer the cached `.mlmodelc`. If it loads, done — this is the steady state once the
///    `.mlpackage` source has been reclaimed.
/// 2. If the cached `.mlmodelc` fails to load (an OS upgrade can invalidate a compiled
///    model), drop it and fall through.
/// 3. If the `.mlpackage` source is present, compile it, cache the `.mlmodelc`, and load.
/// 4. If neither a loadable compiled model nor a source package remains, return
///    [`ClipError::NotAvailable`]. Step 2 already deleted any stale `.mlmodelc`, so
///    [`is_installed`](super::install::is_installed) now reads `false` and the standard
///    download flow refetches the pinned zip — a graceful re-download, never a dead feature.
fn load_tower(model_dir: &Path, tower: &ClipTowerSpec) -> Result<Retained<MLModel>, ClipError> {
    let compiled = compiled_path(model_dir, tower);
    let package = package_path(model_dir, tower);

    if compiled.is_dir() {
        match load_model_at(&file_url(&compiled)) {
            Ok(model) => return Ok(model),
            Err(e) => {
                log::warn!(target: "media_index", "compiled CLIP tower '{}' won't load ({e}); dropping the stale `.mlmodelc`", tower.artifact);
                let _ = drop_compiled(model_dir, tower);
            }
        }
    }

    if package.is_dir() {
        let compiled_url = compile_and_cache(&package, &compiled)?;
        return load_model_at(&compiled_url);
    }

    Err(ClipError::NotAvailable)
}

/// Load a compiled `.mlmodelc` at `url` with `MLComputeUnits::All`.
fn load_model_at(url: &NSURL) -> Result<Retained<MLModel>, ClipError> {
    autoreleasepool(|_| {
        // SAFETY: `new()` constructs a valid, fresh `MLModelConfiguration` (no arguments,
        // no preconditions); the returned handle is retained and owned here.
        let config = unsafe { MLModelConfiguration::new() };
        // SAFETY: `config` is a freshly created, valid configuration; `All` is a valid
        // enum value. Sets the compute units before load so Core ML picks the ANE.
        unsafe { config.setComputeUnits(MLComputeUnits::All) };
        // SAFETY: `url` is a valid file URL to a compiled `.mlmodelc`; `config` is a valid
        // configuration. The `_error` variant returns `Err(NSError)` on failure rather than
        // throwing, so a bad model is a typed error, not a panic.
        let model = unsafe { MLModel::modelWithContentsOfURL_configuration_error(url, &config) }
            .map_err(|e| ClipError::Load(e.to_string()))?;
        Ok(model)
    })
}

/// Compile a `.mlpackage` on-device and cache the resulting `.mlmodelc` at `cache`, returning
/// the URL to load from (the cached copy, or the temp URL if the cache copy failed).
/// Compilation is OS-version specific, so it's never bundled.
fn compile_and_cache(pkg_path: &Path, cache: &Path) -> Result<Retained<NSURL>, ClipError> {
    let pkg_url = file_url(pkg_path);
    // The synchronous compile is deprecated in favor of the async completion-handler
    // variant, but we WANT to block here (the worker thread serializes load anyway), so a
    // completion handler would only add a channel round-trip. `allow(deprecated)` is the
    // documented exception (no-ignored-warnings): the sync form is correct for this use.
    #[allow(
        deprecated,
        reason = "objc2-core-ml deprecates the direct sync form in favor of a heavier async/closure API; the direct form is exactly what the CLIP spike proved and is correct on this serialized worker thread"
    )]
    // SAFETY: `pkg_url` is a valid file URL to an `.mlpackage`; the `_error` variant
    // returns `Err(NSError)` on a bad package rather than throwing. Core ML writes the
    // compiled model to a temporary URL it returns (owned by us, +1 retain).
    let temp = unsafe { MLModel::compileModelAtURL_error(&pkg_url) }.map_err(|e| ClipError::Load(e.to_string()))?;
    let temp_path = url_to_path(&temp).ok_or_else(|| ClipError::Load("compiled model URL had no path".into()))?;
    // Cache the compiled bundle beside the package so later launches skip compilation.
    // Best-effort: if the copy fails, load straight from the temp URL for this session.
    if copy_dir_recursive(&temp_path, cache).is_ok() {
        Ok(file_url(cache))
    } else {
        Ok(temp)
    }
}

/// Whether both towers produce a sane embedding from a zero input — the M5a delete-guard
/// (512-d and all-finite, so a NaN-emitting model keeps its source rather than reclaiming
/// it). Runs one throwaway encode per tower on the worker thread at first load.
fn verify_sane(models: &ClipModels) -> bool {
    let image_ok = is_sane_embedding(&models.encode_image(&vec![0.0f32; IMAGE_PIXELS]));
    let text_ok = is_sane_embedding(&models.encode_text(&vec![0i32; CONTEXT_LENGTH]));
    image_ok && text_ok
}

/// A produced embedding is sane when it's the expected width and holds no NaN/inf.
fn is_sane_embedding(result: &Result<Vec<f32>, ClipError>) -> bool {
    matches!(result, Ok(v) if v.len() == EMBED_DIM && v.iter().all(|x| x.is_finite()))
}

impl ClipModels {
    fn encode_text(&self, ids: &[i32]) -> Result<Vec<f32>, ClipError> {
        autoreleasepool(|_| {
            let arr = int32_multiarray(&[1, CONTEXT_LENGTH as isize], ids)?;
            predict(&self.text, "input_ids", &arr)
        })
    }

    fn encode_image(&self, pixels: &[f32]) -> Result<Vec<f32>, ClipError> {
        autoreleasepool(|_| {
            let arr = float32_multiarray(&[1, 3, IMAGE_SIDE as isize, IMAGE_SIDE as isize], pixels)?;
            predict(&self.image, "image", &arr)
        })
    }
}

/// Run one prediction: wrap `arr` as the named input feature, predict, and read the
/// `"embedding"` output MLMultiArray back into a `Vec<f32>`.
fn predict(model: &MLModel, input_name: &str, arr: &MLMultiArray) -> Result<Vec<f32>, ClipError> {
    let name = NSString::from_str(input_name);
    // SAFETY: `arr` is a valid, fully-initialized MLMultiArray; `featureValueWithMultiArray`
    // wraps it (+1 retain) and never fails for a valid array.
    let value = unsafe { MLFeatureValue::featureValueWithMultiArray(arr) };
    // Build the single-entry input dictionary {input_name: value}. Deref coercion turns the
    // `Retained<MLFeatureValue>` into the `&AnyObject` the dictionary value type wants.
    let value_any: &AnyObject = &value;
    let dict = NSDictionary::<NSString, AnyObject>::from_slices::<NSString>(&[&name], &[value_any]);
    // SAFETY: `dict` is a valid `{NSString: MLFeatureValue}` dictionary (the correct value
    // type for a feature provider); the `_error` variant returns a typed error.
    let provider =
        unsafe { MLDictionaryFeatureProvider::initWithDictionary_error(MLDictionaryFeatureProvider::alloc(), &dict) }
            .map_err(|e| ClipError::Predict(e.to_string()))?;
    let provider_proto = ProtocolObject::from_ref(&*provider);
    // SAFETY: `provider_proto` conforms to MLFeatureProvider; the `_error` predict variant
    // returns `Err(NSError)` on failure rather than throwing.
    let out =
        unsafe { model.predictionFromFeatures_error(provider_proto) }.map_err(|e| ClipError::Predict(e.to_string()))?;
    read_embedding(&out)
}

/// Read the `"embedding"` output feature (an MLMultiArray) into a `Vec<f32>`.
fn read_embedding(out: &ProtocolObject<dyn MLFeatureProvider>) -> Result<Vec<f32>, ClipError> {
    let key = NSString::from_str("embedding");
    // SAFETY: `out` is the valid provider Core ML returned; `featureValueForName` is a
    // plain accessor returning `Option` (null-checked below).
    let value =
        unsafe { out.featureValueForName(&key) }.ok_or_else(|| ClipError::Predict("no 'embedding' output".into()))?;
    // SAFETY: `value` is a valid feature value; `multiArrayValue` returns `Option`
    // (null when the value isn't a multi-array — null-checked).
    let arr =
        unsafe { value.multiArrayValue() }.ok_or_else(|| ClipError::Predict("embedding not a multiarray".into()))?;
    read_f32_multiarray(&arr)
}

/// Build a `[1, 77]`-shaped Int32 MLMultiArray filled with `ids`.
// `dataPointer` is deprecated in favor of the `getBytesWithHandler` closure API, but the
// direct contiguous-pointer access is exactly what the CLIP spike proved
// (`docs/notes/clip-coreml-rust-spike.md`) and is far simpler than a closure round-trip
// for a one-shot fill/read. `allow(deprecated)` is the documented exception.
#[allow(
    deprecated,
    reason = "objc2-core-ml deprecates the direct sync form in favor of a heavier async/closure API; the direct form is exactly what the CLIP spike proved and is correct on this serialized worker thread"
)]
fn int32_multiarray(shape: &[isize], ids: &[i32]) -> Result<Retained<MLMultiArray>, ClipError> {
    let arr = new_multiarray(shape, MLMultiArrayDataType::Int32)?;
    // SAFETY: `dataPointer` is the array's contiguous first-major backing store; we sized
    // it as `shape` (element count == ids.len(), asserted by the caller's fixed shapes), so
    // writing `ids.len()` i32s stays in bounds. The array outlives this write.
    unsafe {
        let ptr = arr.dataPointer().as_ptr().cast::<i32>();
        std::ptr::copy_nonoverlapping(ids.as_ptr(), ptr, ids.len());
    }
    Ok(arr)
}

/// Build a Float32 MLMultiArray of `shape` filled with `values`.
#[allow(
    deprecated,
    reason = "objc2-core-ml deprecates the direct sync form in favor of a heavier async/closure API; the direct form is exactly what the CLIP spike proved and is correct on this serialized worker thread"
)] // direct `dataPointer` fill — see `int32_multiarray`.
fn float32_multiarray(shape: &[isize], values: &[f32]) -> Result<Retained<MLMultiArray>, ClipError> {
    let arr = new_multiarray(shape, MLMultiArrayDataType::Float32)?;
    // SAFETY: as `int32_multiarray`, but f32 — the array is sized to `values.len()`.
    unsafe {
        let ptr = arr.dataPointer().as_ptr().cast::<f32>();
        std::ptr::copy_nonoverlapping(values.as_ptr(), ptr, values.len());
    }
    Ok(arr)
}

fn new_multiarray(shape: &[isize], dtype: MLMultiArrayDataType) -> Result<Retained<MLMultiArray>, ClipError> {
    let numbers: Vec<Retained<NSNumber>> = shape.iter().map(|d| NSNumber::new_isize(*d)).collect();
    let refs: Vec<&NSNumber> = numbers.iter().map(|n| n.as_ref()).collect();
    let shape_arr = NSArray::from_slice(&refs);
    // SAFETY: `shape_arr` is a valid NSArray<NSNumber>; the `_error` initializer returns a
    // typed error on failure. `alloc` gives a fresh uninitialized instance the init consumes.
    unsafe { MLMultiArray::initWithShape_dataType_error(MLMultiArray::alloc(), &shape_arr, dtype) }
        .map_err(|e| ClipError::Predict(e.to_string()))
}

/// Read a Float32 MLMultiArray into a `Vec<f32>` (the output embedding). Assumes a
/// contiguous first-major layout, which Core ML guarantees for a freshly produced output.
#[allow(
    deprecated,
    reason = "objc2-core-ml deprecates the direct sync form in favor of a heavier async/closure API; the direct form is exactly what the CLIP spike proved and is correct on this serialized worker thread"
)] // direct `dataPointer` read — see `int32_multiarray`.
fn read_f32_multiarray(arr: &MLMultiArray) -> Result<Vec<f32>, ClipError> {
    // SAFETY: `count` is the element count; `dataPointer` is the contiguous backing store.
    // We read exactly `count` f32s (the output is Float32 for our towers). The array is
    // retained for the duration of this read.
    let (count, ptr) = unsafe { (arr.count() as usize, arr.dataPointer().as_ptr().cast::<f32>()) };
    if count == 0 {
        return Err(ClipError::Predict("empty embedding".into()));
    }
    let mut out = vec![0f32; count];
    // SAFETY: `ptr` points to `count` contiguous f32s (Core ML's output buffer); copying
    // `count` elements stays in bounds.
    unsafe { std::ptr::copy_nonoverlapping(ptr, out.as_mut_ptr(), count) };
    debug_assert_eq!(count, EMBED_DIM, "CLIP embedding should be {EMBED_DIM}-d");
    Ok(out)
}

// ── URL / path helpers ─────────────────────────────────────────────────────

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

fn url_to_path(url: &NSURL) -> Option<PathBuf> {
    let s = url.path()?;
    Some(PathBuf::from(s.to_string()))
}

/// Recursively copy a directory tree (an `.mlmodelc` bundle) to `dest`.
fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

// ── Process-global worker + model dir ──────────────────────────────────────

use std::sync::OnceLock;

/// The app data dir the towers install under, set once at scheduler start.
static MODEL_DIR: OnceLock<PathBuf> = OnceLock::new();
/// The lazily-spawned worker, created on first encode after [`set_data_dir`].
static WORKER: OnceLock<ClipWorker> = OnceLock::new();

/// Record the app data dir the CLIP model installs under (called at scheduler start), so
/// the worker knows where to load the towers from.
pub fn set_data_dir(data_dir: &Path) {
    let _ = MODEL_DIR.set(clip_model_dir(data_dir));
}

/// The process-global worker, spawned on first use. `None` until [`set_data_dir`] ran.
fn worker() -> Option<&'static ClipWorker> {
    let dir = MODEL_DIR.get()?;
    Some(WORKER.get_or_init(|| ClipWorker::spawn(dir.clone())))
}

/// Encode already-tokenized ids via the worker (query-time text tower).
pub fn encode_text(ids: Vec<i32>) -> Result<Vec<f32>, ClipError> {
    worker().ok_or(ClipError::NotAvailable)?.encode_text(ids)
}

/// Encode a CHW `[0,1]` pixel buffer via the worker (enrichment image tower).
pub fn encode_image(pixels: Vec<f32>) -> Result<Vec<f32>, ClipError> {
    worker().ok_or(ClipError::NotAvailable)?.encode_image(pixels)
}
