//! The process-global runtime flags the enrichment scheduler gates on.
//!
//! - **[`is_enabled`]**: the master "Index image contents" toggle, seeded from
//!   settings at startup (OFF by default) and flipped by the live-apply settings
//!   command. Every pass checks it before doing any work.
//! - **[`scope`]**: WHICH folders a pass may cover — the user's explicit choice
//!   between "only the folders I chose" and "automatically, by folder importance".
//!   The importance threshold is consulted ONLY in the automatic scope.
//! - **[`semantic_search_enabled`]**: the CLIP semantic-search on/off (ON by default),
//!   seeded from settings at startup and live-applied. It gates BOTH the read
//!   (`search_semantic` returns nothing when off) and the CLIP embedding WRITE
//!   (`clip::current_stamp` returns `None` when off, so `needs_clip` is never true and
//!   no pass embeds CLIP), so turning it off stops all new CLIP work at once.
//! - **[`is_cancelled`]**: the emergency stop the indexing memory watchdog sets via
//!   its subsystem-stop hook (media_index shares the ONE resident-memory ceiling,
//!   it does not stand up a second one — see the plan's Resources cross-cutting).
//!   The pass checks it BETWEEN images so it yields promptly under memory pressure.
//!   Enabling the feature clears it, so re-enabling recovers.
//!
//! The enrichment core takes the cancel decision as an argument (a closure), so it
//! stays unit-testable without touching these globals; only the live scheduler
//! reads them.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// The CLIP semantic-search on/off. ON by default: with no model installed it's inert
/// anyway (nothing to embed, no embeddings to search), so defaulting on keeps the
/// download-then-search flow one step. Seeded from `mediaIndex.semanticSearch.enabled`
/// at startup and live-applied by [`set_semantic_search_enabled`].
static SEMANTIC_SEARCH_ENABLED: AtomicBool = AtomicBool::new(true);

/// WHICH folders image indexing may cover — the user's explicit scope choice, an
/// enum rather than a sentinel threshold value so "index nothing but my chosen
/// folders" is a stated mode, never a number a reader has to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexScope {
    /// Index ONLY what the user explicitly chose: the "always index" folder and volume
    /// overrides. Importance (and therefore the threshold) is never consulted, so a
    /// folder nobody named is never enriched. The default.
    ChosenFolders,
    /// Index automatically by folder importance: the overrides PLUS every folder
    /// scoring at or above the importance threshold. The only scope where the slider
    /// means anything.
    ByImportance,
}

impl IndexScope {
    /// Whether this scope consults folder importance at all. `false` for
    /// [`ChosenFolders`](IndexScope::ChosenFolders), which is exactly the
    /// override-only coverage an unscored volume already falls back to — so the
    /// narrow scope reuses that one path instead of adding a parallel gate.
    pub fn consults_importance(self) -> bool {
        self == IndexScope::ByImportance
    }

    /// The stable token this scope persists as in `settings.json`
    /// (`mediaIndex.scope`).
    pub fn as_token(self) -> &'static str {
        match self {
            IndexScope::ChosenFolders => "chosen",
            IndexScope::ByImportance => "importance",
        }
    }

    /// Parse a persisted token. An unknown or absent token reads as
    /// [`DEFAULT_SCOPE`] — a settings file written by a newer build (or a hand-edit)
    /// falls back to the narrow scope, which can only ever index LESS than the user
    /// asked for, never more.
    pub fn from_token(token: &str) -> IndexScope {
        match token {
            "importance" => IndexScope::ByImportance,
            _ => DEFAULT_SCOPE,
        }
    }
}

/// The scope a fresh install starts in: index nothing but the folders the user names.
pub const DEFAULT_SCOPE: IndexScope = IndexScope::ChosenFolders;

/// The current scope, as an [`IndexScope`] discriminant, so the scheduler reads it
/// lock-free on every pass.
static SCOPE: AtomicU8 = AtomicU8::new(scope_bits(DEFAULT_SCOPE));

/// The atomic representation of a scope. A `const fn` so the static above can be
/// initialized from [`DEFAULT_SCOPE`] rather than a repeated literal.
const fn scope_bits(scope: IndexScope) -> u8 {
    match scope {
        IndexScope::ChosenFolders => 0,
        IndexScope::ByImportance => 1,
    }
}

/// The lowest folder-importance level (`0.0..=1.0`) the user wants image-indexed —
/// the importance settings slider's typed value. Stored as `f64` bits in an atomic
/// so the scheduler reads it lock-free on every pass. The default is
/// [`DEFAULT_IMPORTANCE_THRESHOLD`]; the slider live-applies a new value.
static IMPORTANCE_THRESHOLD_BITS: AtomicU64 = AtomicU64::new(DEFAULT_IMPORTANCE_THRESHOLD.to_bits());

/// The default importance threshold before the user touches the slider: `0.0`, i.e.
/// enrich every folder importance scores at all. Importance already floors junk
/// (`node_modules`, caches, hidden/system) to no row, so `0.0` still skips junk while
/// preserving the pre-slider behavior of covering all real folders. The slider raises
/// it to defer low-importance folders.
pub const DEFAULT_IMPORTANCE_THRESHOLD: f64 = 0.0;

/// Set the master toggle. Enabling also clears any prior emergency-stop so the
/// scheduler resumes.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        CANCELLED.store(false, Ordering::SeqCst);
    }
}

/// Whether image indexing is enabled (the master toggle).
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/// Request that in-flight enrichment yield (the memory watchdog's stop hook calls
/// this). Idempotent; cleared by [`set_enabled(true)`](set_enabled).
pub fn request_cancel() {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// Whether an emergency stop is in effect. The pass checks this between images.
pub fn is_cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// Set the CLIP semantic-search toggle (live-applied by the settings control, and seeded
/// at startup). When off, [`clip::current_stamp`](super::clip::current_stamp) returns
/// `None` (so no pass embeds CLIP) and `search_semantic` returns nothing; existing
/// embeddings are kept (turning off is "stop", never "erase" — the delete-model action is
/// the separate explicit reclaim).
pub fn set_semantic_search_enabled(enabled: bool) {
    SEMANTIC_SEARCH_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Whether CLIP semantic search is on (the read gate AND the CLIP-write gate both read
/// this ONE value, so they can't disagree about whether CLIP work happens).
pub fn semantic_search_enabled() -> bool {
    SEMANTIC_SEARCH_ENABLED.load(Ordering::SeqCst)
}

/// Whether an in-flight enrichment pass should STOP promptly. It's the ONE predicate
/// every pass's between-images cancel hook checks (the local full pass, the SMB network
/// pass, and the live tick), true on EITHER of two independent reasons:
///
/// - the memory watchdog fired an emergency stop ([`is_cancelled`]), OR
/// - the user turned the master "Index image contents" toggle OFF ([`is_enabled`] is
///   false), so a pass already running (e.g. a NAS pass at image 74 of 31,890) yields
///   within a few images instead of grinding to completion after the user said stop.
///
/// The two reasons stay SEPARATE at the atomic level: disabling touches no atomic here,
/// it's observed live off [`is_enabled`], so [`is_cancelled`] / [`request_cancel`] keep
/// their exact watchdog meaning and re-enabling can never leave a stuck flag —
/// [`set_enabled(true)`](set_enabled) clears the emergency stop and the scheduler kicks
/// fresh passes, and the disable input is simply `is_enabled() == true` again. Stopping
/// reuses the existing cancel exit (rows kept, GC skipped): disabling is "stop
/// processing", never "erase".
pub fn should_stop() -> bool {
    is_cancelled() || !is_enabled()
}

/// Set the indexing scope (live-applied by the scope control in settings, and seeded
/// at startup).
pub fn set_scope(next: IndexScope) {
    SCOPE.store(scope_bits(next), Ordering::SeqCst);
}

/// The current indexing scope. Every coverage decision (a pass's gate, the settings
/// preview, the reclaim partition) reads this ONE value, so they can't disagree about
/// what "covered" means.
pub fn scope() -> IndexScope {
    match SCOPE.load(Ordering::SeqCst) {
        1 => IndexScope::ByImportance,
        _ => DEFAULT_SCOPE,
    }
}

/// The scope to start in, given the persisted `mediaIndex.scope` token (absent on any
/// install predating the setting) and the persisted master toggle.
///
/// A stated scope always wins. With NO stated scope we key on whether image indexing
/// was already ON: someone running it today is running the automatic behavior, and
/// silently narrowing their indexed set at launch would be a change they never asked
/// for. Everyone else (the overwhelming majority, since the feature is off by default)
/// starts at [`DEFAULT_SCOPE`]. Idempotent, so it's safe to re-derive every launch
/// until the settings migration writes the key.
pub fn scope_from_settings(scope: Option<&str>, was_enabled: Option<bool>) -> IndexScope {
    match scope {
        Some(token) => IndexScope::from_token(token),
        None if was_enabled == Some(true) => IndexScope::ByImportance,
        None => DEFAULT_SCOPE,
    }
}

/// Whether a scope change from `previous` to `next` BROADENS coverage, the only
/// direction that should kick an immediate pass: switching to
/// [`ByImportance`](IndexScope::ByImportance) newly covers every folder above the
/// threshold, so it starts enriching now. Narrowing merely stops future work (the
/// already-indexed rows persist, forward-only, until the user reclaims them), so a kick
/// there would re-walk the index for nothing.
pub fn scope_broadened(previous: IndexScope, next: IndexScope) -> bool {
    previous == IndexScope::ChosenFolders && next == IndexScope::ByImportance
}

/// Set the importance threshold (clamped to `0.0..=1.0`). Seeded from settings at
/// startup and live-applied by the slider's settings command.
pub fn set_importance_threshold(threshold: f64) {
    IMPORTANCE_THRESHOLD_BITS.store(threshold.clamp(0.0, 1.0).to_bits(), Ordering::SeqCst);
}

/// The current importance threshold (`0.0..=1.0`). The scheduler enriches a folder
/// only when its importance is at or above this (or an override covers it).
pub fn importance_threshold() -> f64 {
    f64::from_bits(IMPORTANCE_THRESHOLD_BITS.load(Ordering::SeqCst))
}

/// Whether a threshold change from `previous` to `next` is a DECREASE (coverage
/// broadens). Only a decrease should kick an immediate pass: the newly-covered
/// folders start enriching now, while a raise merely defers future work
/// (forward-only — nothing to enrich now), so kicking on a raise would re-walk the
/// index for nothing. Both operands are already-clamped stored values, so
/// the comparison can't be fooled by an out-of-range incoming request.
pub fn threshold_decreased(previous: f64, next: f64) -> bool {
    next < previous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_indexes_only_chosen_folders() {
        assert_eq!(DEFAULT_SCOPE, IndexScope::ChosenFolders);
        assert!(!IndexScope::ChosenFolders.consults_importance());
        assert!(IndexScope::ByImportance.consults_importance());
    }

    #[test]
    fn a_scope_token_round_trips_and_an_unknown_one_falls_back_narrow() {
        for scope in [IndexScope::ChosenFolders, IndexScope::ByImportance] {
            assert_eq!(IndexScope::from_token(scope.as_token()), scope);
        }
        // A token from a newer build (or a hand-edit) can only ever index LESS.
        assert_eq!(IndexScope::from_token("everything"), IndexScope::ChosenFolders);
        assert_eq!(IndexScope::from_token(""), IndexScope::ChosenFolders);
    }

    #[test]
    fn an_existing_user_keeps_the_automatic_scope_a_new_one_starts_narrow() {
        // Nobody stated a scope, but image indexing is already ON ⇒ keep today's
        // behavior; narrowing a live user's indexed set at launch is a change they
        // never asked for.
        assert_eq!(scope_from_settings(None, Some(true)), IndexScope::ByImportance);
        // Never enabled (the overwhelming majority: the feature is off by default) ⇒
        // the new default.
        assert_eq!(scope_from_settings(None, Some(false)), IndexScope::ChosenFolders);
        assert_eq!(scope_from_settings(None, None), IndexScope::ChosenFolders);
        // A stated scope always wins, including a stated narrow one on an enabled install.
        assert_eq!(
            scope_from_settings(Some("chosen"), Some(true)),
            IndexScope::ChosenFolders
        );
        assert_eq!(
            scope_from_settings(Some("importance"), Some(false)),
            IndexScope::ByImportance
        );
    }

    #[test]
    fn only_broadening_the_scope_kicks() {
        // Narrow → automatic newly covers every above-threshold folder ⇒ kick.
        assert!(scope_broadened(IndexScope::ChosenFolders, IndexScope::ByImportance));
        // Automatic → narrow only stops future work (rows persist) ⇒ no kick.
        assert!(!scope_broadened(IndexScope::ByImportance, IndexScope::ChosenFolders));
        // No change ⇒ no kick.
        assert!(!scope_broadened(IndexScope::ChosenFolders, IndexScope::ChosenFolders));
        assert!(!scope_broadened(IndexScope::ByImportance, IndexScope::ByImportance));
    }

    #[test]
    fn only_a_decrease_kicks() {
        // A decrease broadens coverage ⇒ kick.
        assert!(threshold_decreased(0.6, 0.2));
        assert!(threshold_decreased(0.2, 0.0));
        // A raise defers future work only ⇒ no kick.
        assert!(!threshold_decreased(0.2, 0.6));
        assert!(!threshold_decreased(0.0, 0.8));
        // No change ⇒ no kick.
        assert!(!threshold_decreased(0.4, 0.4));
    }
}
