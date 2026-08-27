//! Which CLIP tower a job needs, and the lazy per-tower load + M5a source reclaim around it.
//!
//! The two towers are independently loadable, verifiable, and reclaimable, and they cost
//! wildly different amounts of resident memory (`CLAUDE.md` § "What holding the towers
//! costs"), so each loads on the FIRST job that needs it: an enrichment pass that never
//! encodes a query never materializes the text tower, and a session that only ever searches
//! never materializes the image one.
//!
//! The Core ML side lives in `macos.rs` behind the [`TowerEngine`] seam, which is what lets
//! the load / verify / reclaim RULES be tested with no model on disk and no ANE.

use std::path::{Path, PathBuf};

use super::ClipError;
use super::install::{ClipTower, package_path, reclaim_source_package};
use super::tokenizer::CONTEXT_LENGTH;

/// The CLIP embedding dimensionality (OpenAI CLIP ViT-B/32). Both towers output this.
pub(crate) const EMBED_DIM: usize = 512;
/// The image tower input side (224×224 RGB, CHW).
pub(crate) const IMAGE_SIDE: usize = 224;
/// The image tower input element count (`1 × 3 × 224 × 224`).
pub(crate) const IMAGE_PIXELS: usize = 3 * IMAGE_SIDE * IMAGE_SIDE;

/// The input to one CLIP encode. The variant IS the tower: a token-id sequence only ever
/// reaches the text tower and a pixel buffer only ever the image tower, so no caller can
/// route a job to the wrong half and no `tower` argument can disagree with the payload.
pub(crate) enum ClipInput<'a> {
    /// Tokenized query ids (length [`CONTEXT_LENGTH`]) for the text tower.
    Ids(&'a [i32]),
    /// A CHW `[0,1]` pixel buffer (length [`IMAGE_PIXELS`]) for the image tower.
    Pixels(&'a [f32]),
}

impl ClipInput<'_> {
    /// The tower this input belongs to.
    pub(crate) fn tower(&self) -> ClipTower {
        match self {
            ClipInput::Ids(_) => ClipTower::Text,
            ClipInput::Pixels(_) => ClipTower::Image,
        }
    }

    /// The all-zero input `which` takes, for the throwaway encode that guards the M5a
    /// reclaim.
    fn zeros_for(which: ClipTower) -> ZeroInput {
        match which {
            ClipTower::Image => ZeroInput::Pixels(vec![0.0f32; IMAGE_PIXELS]),
            ClipTower::Text => ZeroInput::Ids(vec![0i32; CONTEXT_LENGTH]),
        }
    }
}

/// An owned all-zero input, kept alive while [`ClipInput`] borrows it.
enum ZeroInput {
    Ids(Vec<i32>),
    Pixels(Vec<f32>),
}

impl ZeroInput {
    fn as_input(&self) -> ClipInput<'_> {
        match self {
            ZeroInput::Ids(v) => ClipInput::Ids(v),
            ZeroInput::Pixels(v) => ClipInput::Pixels(v),
        }
    }
}

/// Loads and runs one CLIP tower. Core ML implements this in `macos.rs`; a test substitutes
/// a fake, which is what makes the lazy-load and reclaim rules below testable without the
/// ~267 MB model and without an ANE.
pub(crate) trait TowerEngine {
    /// One loaded tower.
    type Model;

    /// Load `which` from `model_dir`, compiling its `.mlpackage` if no usable compiled
    /// model is cached yet.
    fn load(&self, model_dir: &Path, which: ClipTower) -> Result<Self::Model, ClipError>;

    /// Run one encode through an already-loaded tower. `input`'s variant matches the tower
    /// the caller loaded ([`ClipInput::tower`]).
    fn encode(&self, model: &Self::Model, input: ClipInput<'_>) -> Result<Vec<f32>, ClipError>;
}

/// The worker's two towers, each loaded on the first job that needs it and then held for
/// the process. A tower that fails to load caches its error, so a broken install costs one
/// failed load rather than one per job, and a broken TEXT tower still leaves enrichment's
/// image tower working.
pub(crate) struct Towers<E: TowerEngine> {
    engine: E,
    model_dir: PathBuf,
    image: Option<Result<E::Model, ClipError>>,
    text: Option<Result<E::Model, ClipError>>,
}

impl<E: TowerEngine> Towers<E> {
    /// Two unloaded slots. Nothing touches disk until the first [`encode`](Self::encode).
    pub(crate) fn new(engine: E, model_dir: PathBuf) -> Self {
        Towers {
            engine,
            model_dir,
            image: None,
            text: None,
        }
    }

    /// Encode `input` through the tower it belongs to, loading that tower on first use.
    pub(crate) fn encode(&mut self, input: ClipInput<'_>) -> Result<Vec<f32>, ClipError> {
        let which = input.tower();
        let Towers {
            engine,
            model_dir,
            image,
            text,
        } = self;
        let slot = match which {
            ClipTower::Image => image,
            ClipTower::Text => text,
        };
        let model = slot
            .get_or_insert_with(|| load_and_reclaim(engine, model_dir, which))
            .as_ref()
            .map_err(Clone::clone)?;
        engine.encode(model, input)
    }

    /// Whether `which` has been loaded (or tried and cached as failed) yet.
    #[cfg(test)]
    fn is_loaded(&self, which: ClipTower) -> bool {
        match which {
            ClipTower::Image => self.image.is_some(),
            ClipTower::Text => self.text.is_some(),
        }
    }
}

/// Load one tower and settle its M5a source reclaim in the same step.
///
/// **The reclaim is per tower, and it never rides on another tower's word.** A tower's
/// `.mlpackage` source is deleted only once THAT tower has loaded AND encoded a sane
/// embedding through its own compiled model, so a tower nobody asked for keeps its source,
/// and a tower whose zero-input encode comes back unusable (the all-NaN a bad palettization
/// emits) keeps its source so a recompile stays possible.
///
/// The guard encode runs only while there is still a source to reclaim, so the steady state
/// (every launch after the first) pays nothing for it and no user's first search wears it.
fn load_and_reclaim<E: TowerEngine>(engine: &E, model_dir: &Path, which: ClipTower) -> Result<E::Model, ClipError> {
    let spec = which.spec();
    let model = engine.load(model_dir, which)?;
    if !package_path(model_dir, spec).is_dir() {
        return Ok(model);
    }
    if !encodes_sanely(engine, &model, which) {
        log::warn!(target: "media_index", "CLIP tower '{}' encoded an unusable embedding; keeping its `.mlpackage` source so a recompile stays possible", spec.artifact);
        return Ok(model);
    }
    match reclaim_source_package(model_dir, spec) {
        Ok(true) => {
            log::info!(target: "media_index", "reclaimed CLIP source package '{}' after verified compile", spec.artifact)
        }
        Ok(false) => {}
        Err(e) => log::warn!(target: "media_index", "reclaim of CLIP source '{}' failed: {e}", spec.artifact),
    }
    Ok(model)
}

/// Whether a freshly-loaded tower turns an all-zero input into a sane embedding — the M5a
/// delete-guard. One throwaway encode through the tower that is about to lose its source.
fn encodes_sanely<E: TowerEngine>(engine: &E, model: &E::Model, which: ClipTower) -> bool {
    let zeros = ClipInput::zeros_for(which);
    is_sane_embedding(&engine.encode(model, zeros.as_input()))
}

/// A produced embedding is sane when it's the expected width and holds no NaN/inf.
fn is_sane_embedding(result: &Result<Vec<f32>, ClipError>) -> bool {
    matches!(result, Ok(v) if v.len() == EMBED_DIM && v.iter().all(|x| x.is_finite()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::media_index::clip::install::{ClipTowerSpec, clip_model_dir, compiled_path};

    /// A tower engine with no Core ML behind it: it records every load and every encode by
    /// tower, and can be told to emit an unusable embedding or to fail one tower's load, so
    /// the rules in this module are exercised with no model on disk.
    struct FakeEngine {
        loads: RefCell<Vec<ClipTower>>,
        encodes: RefCell<Vec<ClipTower>>,
        /// The tower whose encodes come back all-NaN (a bad palettization), if any.
        nan_from: Option<ClipTower>,
        /// The tower whose load fails, if any.
        load_fails: Option<ClipTower>,
    }

    impl FakeEngine {
        fn healthy() -> FakeEngine {
            FakeEngine {
                loads: RefCell::new(Vec::new()),
                encodes: RefCell::new(Vec::new()),
                nan_from: None,
                load_fails: None,
            }
        }

        fn emitting_nan_from(which: ClipTower) -> FakeEngine {
            FakeEngine {
                nan_from: Some(which),
                ..FakeEngine::healthy()
            }
        }

        fn failing_to_load(which: ClipTower) -> FakeEngine {
            FakeEngine {
                load_fails: Some(which),
                ..FakeEngine::healthy()
            }
        }
    }

    impl TowerEngine for FakeEngine {
        /// A "loaded tower" is just the role it plays, so an encode can report which
        /// tower's model it ran through.
        type Model = ClipTower;

        fn load(&self, _model_dir: &Path, which: ClipTower) -> Result<ClipTower, ClipError> {
            self.loads.borrow_mut().push(which);
            if self.load_fails == Some(which) {
                return Err(ClipError::Load("fake load failure".into()));
            }
            Ok(which)
        }

        fn encode(&self, model: &ClipTower, _input: ClipInput<'_>) -> Result<Vec<f32>, ClipError> {
            self.encodes.borrow_mut().push(*model);
            let value = if self.nan_from == Some(*model) { f32::NAN } else { 0.5 };
            Ok(vec![value; EMBED_DIM])
        }
    }

    /// Make a fake tower directory (`.mlpackage` or `.mlmodelc`) with one file inside.
    fn make_dir(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("weights.bin"), b"x").unwrap();
    }

    /// A model dir holding both towers' `.mlpackage` source AND compiled `.mlmodelc` — the
    /// state right after a first download plus compile, when the reclaim is pending.
    fn model_dir_pending_reclaim(root: &Path) -> PathBuf {
        let model_dir = clip_model_dir(root);
        for which in [ClipTower::Image, ClipTower::Text] {
            make_dir(&package_path(&model_dir, which.spec()));
            make_dir(&compiled_path(&model_dir, which.spec()));
        }
        model_dir
    }

    fn image_pixels() -> Vec<f32> {
        vec![0.25f32; IMAGE_PIXELS]
    }

    fn query_ids() -> Vec<i32> {
        vec![7i32; CONTEXT_LENGTH]
    }

    #[test]
    fn an_enrichment_only_session_never_loads_the_text_tower() {
        let dir = tempfile::tempdir().unwrap();
        let model_dir = model_dir_pending_reclaim(dir.path());
        let mut towers = Towers::new(FakeEngine::healthy(), model_dir);

        for _ in 0..3 {
            towers.encode(ClipInput::Pixels(&image_pixels())).unwrap();
        }

        assert_eq!(
            *towers.engine.loads.borrow(),
            vec![ClipTower::Image],
            "three image encodes load the image tower once and the text tower never"
        );
        assert!(!towers.is_loaded(ClipTower::Text), "the text tower stays unloaded");
    }

    #[test]
    fn a_search_only_session_never_loads_the_image_tower() {
        let dir = tempfile::tempdir().unwrap();
        let model_dir = model_dir_pending_reclaim(dir.path());
        let mut towers = Towers::new(FakeEngine::healthy(), model_dir);

        towers.encode(ClipInput::Ids(&query_ids())).unwrap();

        assert_eq!(*towers.engine.loads.borrow(), vec![ClipTower::Text]);
        assert!(!towers.is_loaded(ClipTower::Image), "the image tower stays unloaded");
    }

    #[test]
    fn only_the_tower_that_actually_verified_loses_its_source_package() {
        // The trap a lazy load sets: reclaiming on a pair-wise precondition would either
        // never fire (trading 246 MB of RAM for 550 MB of permanent disk) or fire for a
        // tower that never proved itself. Each source goes on its OWN tower's word.
        let dir = tempfile::tempdir().unwrap();
        let model_dir = model_dir_pending_reclaim(dir.path());
        let mut towers = Towers::new(FakeEngine::healthy(), model_dir.clone());

        towers.encode(ClipInput::Pixels(&image_pixels())).unwrap();

        assert!(
            !package_path(&model_dir, ClipTower::Image.spec()).is_dir(),
            "the image tower loaded and verified, so its source is reclaimed"
        );
        assert!(
            package_path(&model_dir, ClipTower::Text.spec()).is_dir(),
            "the text tower never loaded, so its source survives for a later compile"
        );
        assert!(
            compiled_path(&model_dir, ClipTower::Image.spec()).is_dir(),
            "the compiled model is what the worker keeps"
        );
    }

    #[test]
    fn a_tower_that_encodes_nan_keeps_its_source_package() {
        let dir = tempfile::tempdir().unwrap();
        let model_dir = model_dir_pending_reclaim(dir.path());
        let mut towers = Towers::new(FakeEngine::emitting_nan_from(ClipTower::Image), model_dir.clone());

        towers.encode(ClipInput::Pixels(&image_pixels())).unwrap();

        assert!(
            package_path(&model_dir, ClipTower::Image.spec()).is_dir(),
            "an unusable embedding keeps the source so a recompile stays possible"
        );
    }

    #[test]
    fn the_guard_encode_is_skipped_once_there_is_no_source_left_to_reclaim() {
        // The post-reclaim steady state: compiled model only. A load must not spend a
        // throwaway encode it can no longer act on — that cost would land inside the
        // user's first search.
        let dir = tempfile::tempdir().unwrap();
        let model_dir = clip_model_dir(dir.path());
        make_dir(&compiled_path(&model_dir, ClipTower::Text.spec()));
        let mut towers = Towers::new(FakeEngine::healthy(), model_dir);

        towers.encode(ClipInput::Ids(&query_ids())).unwrap();

        assert_eq!(
            *towers.engine.encodes.borrow(),
            vec![ClipTower::Text],
            "one encode, the caller's own — no guard encode when nothing can be reclaimed"
        );
    }

    #[test]
    fn a_load_failure_is_cached_and_scoped_to_the_tower_that_failed() {
        let dir = tempfile::tempdir().unwrap();
        let model_dir = model_dir_pending_reclaim(dir.path());
        let mut towers = Towers::new(FakeEngine::failing_to_load(ClipTower::Text), model_dir.clone());

        assert!(matches!(
            towers.encode(ClipInput::Ids(&query_ids())),
            Err(ClipError::Load(_))
        ));
        assert!(
            towers.encode(ClipInput::Pixels(&image_pixels())).is_ok(),
            "a dead text tower leaves enrichment's image tower working"
        );
        assert!(
            towers.encode(ClipInput::Ids(&query_ids())).is_err(),
            "the failure is cached, not retried per job"
        );
        assert_eq!(
            *towers.engine.loads.borrow(),
            vec![ClipTower::Text, ClipTower::Image],
            "one load attempt per tower"
        );
        assert!(
            package_path(&model_dir, ClipTower::Text.spec()).is_dir(),
            "a tower that never loaded never reclaims"
        );
    }

    #[test]
    fn an_input_names_the_tower_that_takes_it() {
        assert_eq!(ClipInput::Ids(&[0; CONTEXT_LENGTH]).tower(), ClipTower::Text);
        assert_eq!(ClipInput::Pixels(&[0.0; 4]).tower(), ClipTower::Image);
    }

    #[test]
    fn a_wrong_width_or_non_finite_embedding_is_not_sane() {
        assert!(is_sane_embedding(&Ok(vec![0.1; EMBED_DIM])));
        assert!(!is_sane_embedding(&Ok(vec![0.1; EMBED_DIM - 1])), "wrong width");
        let mut nan = vec![0.1; EMBED_DIM];
        nan[3] = f32::NAN;
        assert!(!is_sane_embedding(&Ok(nan)), "a NaN anywhere is unusable");
        assert!(!is_sane_embedding(&Err(ClipError::NotAvailable)));
    }

    /// Keeps the fake honest about the shape the real specs have.
    #[test]
    fn every_tower_has_a_distinct_source_and_compiled_path() {
        let model_dir = Path::new("/tmp/model");
        let paths: Vec<PathBuf> = [ClipTower::Image, ClipTower::Text]
            .iter()
            .flat_map(|w| {
                let spec: &ClipTowerSpec = w.spec();
                [package_path(model_dir, spec), compiled_path(model_dir, spec)]
            })
            .collect();
        let mut unique = paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), paths.len(), "no two towers share an on-disk path");
    }
}
