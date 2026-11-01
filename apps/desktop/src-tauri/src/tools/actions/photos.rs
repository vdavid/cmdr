//! The `search_photos` tool: photo search by description, in-image text, or tag.
//!
//! Shared by both consumers (agent decision D49): the in-app Ask Cmdr agent AND external
//! MCP clients dispatch this one registry entry. It only SHAPES the result of the
//! shipped `media_index` read API (`MediaIndex::search_semantic` / `search_ocr` /
//! `images_with_tag`) — the reuse-the-core rule — and reuses `media_index`'s own
//! `volume_state` derivation for coverage honesty rather than deriving a second copy.
//!
//! ## Privacy: what egresses is derived TEXT, not "just metadata"
//!
//! When the in-app agent runs against a cloud provider, the paths AND the in-image OCR
//! snippet / tag this returns are sensitive derived content (a passport scan's OCR
//! snippet IS the passport number). The cloud AI consent gate covers that egress, and its
//! disclosure names it (the Ask Cmdr part of `ai.cloudConsent.*`, `ai/cloud_consent.rs`). What
//! NEVER crosses is
//! image bytes: [`PhotoHit`] is text-only by construction (string/number fields), so the
//! tool structurally can't hand a provider a thumbnail or pixel buffer — pinned by
//! `tests::photo_hit_is_text_only_no_byte_fields`.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::{ToolError, ToolResult};
use crate::commands::media_index::MediaIndexVolumeState;
use crate::security_log::ContentTool;
use crate::tools::resources::volumes::{VolumeKind, snapshot_volumes};
use crate::tools::sensitive::{Protected, classify, note_refusal, refuse_private_key_text};
use cmdr_index::media_index::read::{MediaIndex, OcrHit, SemanticHit, TagHit};

/// The default hit cap when the caller doesn't specify one, and the hard ceiling on any
/// caller value (an agent never needs a huge grid, and it bounds the payload the LLM reads).
const DEFAULT_LIMIT: usize = 30;
const MAX_LIMIT: usize = 200;
/// How far one volume's read may widen to make up for hidden hits. A folder of protected
/// images past this leaves the answer short, and `protectedHidden` says why.
const MAX_HIDING_FETCH: usize = 4 * MAX_LIMIT;

/// Honest note when image indexing is off entirely.
const OFF_NOTE: &str = "Image indexing is off, so there are no photos to search. The user can turn it on under Settings › AI › Image search.";
/// Honest note when a description (semantic) search was asked for but no CLIP model is installed.
const MODEL_MISSING_NOTE: &str = "Description search needs the on-device photo-search model, which isn't installed. The user can download it under Settings › AI › Image search, or search by in-image text (mode: ocr) instead.";

// ── Result DTOs ─────────────────────────────────────────────────────────────

/// One photo-search hit. TEXT-ONLY by construction: every field is a string or a number,
/// so the tool can't hand a provider image bytes or a thumbnail (the no-bytes property).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoHit {
    /// The matched image's absolute (stored) path.
    pub path: String,
    /// The volume the image lives on (its media-index id).
    pub volume: String,
    /// Why it matched, typed: `description` (whole-image CLIP match), `text` (OCR keyword),
    /// or `tag` (Vision tag). Never a free-text classification the caller must parse.
    pub match_kind: String,
    /// The match strength, when the mode has one: CLIP cosine (`description`) or tag
    /// confidence (`tag`). Absent for a `text` (OCR keyword) match, which is rank-ordered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    /// The in-image text that matched (`text` mode: the OCR snippet with `[`/`]` around the
    /// matched terms). Absent for `description` and `tag` matches. This IS image-derived
    /// content — see the module's privacy note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_text: Option<String>,
}

/// Per-volume coverage honesty: whether the volume is still indexing and how far along, so
/// the model can relay "still indexing, results may be incomplete" instead of a confident
/// empty. Derived from `media_index`'s own [`MediaIndexVolumeState`] — one source, no second copy.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCoverage {
    pub volume: String,
    /// Whether an enrichment pass is running for this volume right now.
    pub indexing: bool,
    /// How many images are already enriched (searchable) on this volume.
    pub enriched_count: u64,
    /// How many images qualify for enrichment per the drive index — the honest denominator.
    /// `None` when the index isn't ready (offline / still scanning).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualifying_count: Option<u64>,
    /// `true` when this volume's coverage is a lower bound (still indexing, or fewer enriched
    /// than qualify), so any empty or short result may be incomplete.
    pub incomplete: bool,
}

/// The tool result. A typed status the model relays honestly — never a bare empty list that
/// hides "indexing is off" or "the model isn't installed".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SearchPhotosResult {
    /// Image indexing is off, so nothing is searchable.
    ImageIndexingOff { note: String },
    /// A `semantic` (description) search was requested but no CLIP model is installed.
    SemanticModelNotInstalled { note: String },
    /// A normal answer (possibly empty), with per-volume coverage.
    Ok {
        /// The effective mode used: `semantic`, `ocr`, `tag`, or `semantic+ocr`.
        mode: String,
        hits: Vec<PhotoHit>,
        /// How many hits the search found, before any size cut.
        total: usize,
        /// How many `hits` carries.
        returned: usize,
        /// `true` when only the first `returned` of `total` hits fit one tool result. Ask
        /// with a smaller `limit`, or a narrower query, for the rest.
        truncated: bool,
        coverage: Vec<VolumeCoverage>,
        /// A coverage caveat the model should relay, when one applies (degraded to OCR
        /// because no model is installed, or a searched volume is still indexing).
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        /// Hits the sensitive-content gate dropped, since a snippet in a hit would leak. They
        /// don't count in `total`.
        #[serde(rename = "protectedHidden", skip_serializing_if = "is_zero")]
        protected_hidden: usize,
    },
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

// ── Mode resolution (pure) ────────────────────────────────────────────────────

/// The mode the caller asked for, parsed from the raw string into a typed value at the
/// boundary (an unknown value is a hard param error, never a silent fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestedMode {
    /// No `mode`: compose semantic + OCR like the search UI (semantic leads).
    Auto,
    Semantic,
    Ocr,
    Tag,
}

fn parse_requested_mode(raw: Option<&str>) -> Result<RequestedMode, ToolError> {
    match raw {
        None => Ok(RequestedMode::Auto),
        Some("semantic") => Ok(RequestedMode::Semantic),
        Some("ocr") => Ok(RequestedMode::Ocr),
        Some("tag") => Ok(RequestedMode::Tag),
        Some(other) => Err(ToolError::invalid_params(format!(
            "Unknown mode '{other}'. Use 'semantic' (by description), 'ocr' (in-image text), or 'tag'."
        ))),
    }
}

/// The concrete search strategy after weighing the requested mode against model availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectiveMode {
    /// Compose CLIP description + OCR keyword, semantic hits leading (the UI's default).
    SemanticThenOcr,
    SemanticOnly,
    OcrOnly,
    Tag,
}

impl EffectiveMode {
    fn uses_semantic(self) -> bool {
        matches!(self, EffectiveMode::SemanticThenOcr | EffectiveMode::SemanticOnly)
    }
    fn uses_ocr(self) -> bool {
        matches!(self, EffectiveMode::SemanticThenOcr | EffectiveMode::OcrOnly)
    }
    /// The wire token for the result's `mode` field.
    fn wire(self) -> &'static str {
        match self {
            EffectiveMode::SemanticThenOcr => "semantic+ocr",
            EffectiveMode::SemanticOnly => "semantic",
            EffectiveMode::OcrOnly => "ocr",
            EffectiveMode::Tag => "tag",
        }
    }
}

/// The outcome of resolving the requested mode against whether a CLIP model is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeResolution {
    /// Run this strategy; `degraded_to_ocr` is `true` when Auto wanted semantic but had no model.
    Search { mode: EffectiveMode, degraded_to_ocr: bool },
    /// An explicit `semantic` request with no model installed — the caller returns the honest status.
    SemanticModelMissing,
}

/// Resolve the effective strategy. Mirrors the search UI: Auto composes semantic + OCR and
/// gracefully degrades to OCR-only with no model; an EXPLICIT semantic request with no model
/// is an honest "model not installed" rather than a silent OCR swap.
fn resolve_effective_mode(requested: RequestedMode, model_installed: bool) -> ModeResolution {
    match requested {
        RequestedMode::Auto if model_installed => ModeResolution::Search {
            mode: EffectiveMode::SemanticThenOcr,
            degraded_to_ocr: false,
        },
        RequestedMode::Auto => ModeResolution::Search {
            mode: EffectiveMode::OcrOnly,
            degraded_to_ocr: true,
        },
        RequestedMode::Semantic if model_installed => ModeResolution::Search {
            mode: EffectiveMode::SemanticOnly,
            degraded_to_ocr: false,
        },
        RequestedMode::Semantic => ModeResolution::SemanticModelMissing,
        RequestedMode::Ocr => ModeResolution::Search {
            mode: EffectiveMode::OcrOnly,
            degraded_to_ocr: false,
        },
        RequestedMode::Tag => ModeResolution::Search {
            mode: EffectiveMode::Tag,
            degraded_to_ocr: false,
        },
    }
}

// ── Hit shaping (pure) ────────────────────────────────────────────────────────

/// Shape one volume's raw read-API hits into typed [`PhotoHit`]s per the effective mode.
/// For `SemanticThenOcr`, semantic (description) hits lead, then OCR (text) hits whose path
/// a semantic hit didn't already cover (dedup by path) — the search UI's exact composition.
fn merge_hits(
    volume: &str,
    mode: EffectiveMode,
    semantic: Vec<SemanticHit>,
    ocr: Vec<OcrHit>,
    tags: Vec<TagHit>,
) -> Vec<PhotoHit> {
    let mut hits = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if mode.uses_semantic() {
        for h in semantic {
            seen.insert(h.path.clone());
            hits.push(PhotoHit {
                path: h.path,
                volume: volume.to_string(),
                match_kind: "description".to_string(),
                score: Some(h.score),
                match_text: None,
            });
        }
    }
    if mode.uses_ocr() {
        for h in ocr {
            if seen.insert(h.path.clone()) {
                hits.push(PhotoHit {
                    path: h.path,
                    volume: volume.to_string(),
                    match_kind: "text".to_string(),
                    score: None,
                    match_text: Some(h.snippet),
                });
            }
        }
    }
    if mode == EffectiveMode::Tag {
        for h in tags {
            hits.push(PhotoHit {
                path: h.path,
                volume: volume.to_string(),
                match_kind: "tag".to_string(),
                score: Some(h.score),
                match_text: None,
            });
        }
    }
    hits
}

/// Order hits across volumes (score-bearing first, highest first; text hits keep their
/// per-volume rank order) and cap at `limit`. Stable, so within-volume order survives.
fn sort_and_cap(mut hits: Vec<PhotoHit>, limit: usize) -> Vec<PhotoHit> {
    hits.sort_by(|a, b| {
        let (sa, sb) = (
            a.score.unwrap_or(f32::NEG_INFINITY),
            b.score.unwrap_or(f32::NEG_INFINITY),
        );
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    hits
}

/// Derive the per-volume coverage row from `media_index`'s own state. A volume is
/// `incomplete` when a pass is running, or fewer images are enriched than qualify. Shared
/// with the `image_facts` tool so the two can't drift apart on honesty.
pub(super) fn derive_coverage(volume: &str, state: &MediaIndexVolumeState) -> VolumeCoverage {
    // The denominator must match what the current scope actually covers, not the whole
    // volume. In the "only folders I choose" scope a fully-indexed pair of folders is 4
    // enriched against ~230k qualifying images drive-wide, which would report incomplete
    // forever and make the agent tell the user their folder isn't indexed when it is.
    // `covered_qualifying_count` is the scope-aware count the settings progress line
    // already uses; fall back to the volume total only when it isn't available.
    let denominator = state.covered_qualifying_count.or(state.qualifying_count);
    let incomplete = state.indexing || denominator.is_some_and(|q| state.enriched_count < q);
    VolumeCoverage {
        volume: volume.to_string(),
        indexing: state.indexing,
        enriched_count: state.enriched_count,
        qualifying_count: denominator,
        incomplete,
    }
}

/// The coverage caveat to relay, if any: OCR degradation leads (it changes what was
/// searched), then a still-indexing note when any searched volume is incomplete.
/// `image_facts` shares this with `degraded_to_ocr: false` (it never runs a CLIP query).
pub(super) fn build_note(degraded_to_ocr: bool, coverage: &[VolumeCoverage]) -> Option<String> {
    if degraded_to_ocr {
        return Some(
            "No photo-search model is installed, so this searched in-image text only, not descriptions. \
             The user can add description search under Settings › AI › Image search."
                .to_string(),
        );
    }
    if coverage.iter().any(|c| c.incomplete) {
        return Some("Some volumes are still indexing, so these results may be incomplete.".to_string());
    }
    None
}

// ── Schema + handler ──────────────────────────────────────────────────────────

pub fn search_photos_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "What to look for: a scene description (semantic), words expected IN the image (ocr), or one object/scene label (tag)." },
            "volumeId": { "type": "string", "description": "One volume id (see list_volumes); omit for every indexed local and SMB volume." },
            "mode": { "type": "string", "enum": ["semantic", "ocr", "tag"], "description": "semantic: by visual description. ocr: by text recognized in the image. tag: by a Vision label. Omit to combine semantic + ocr, best for most photo questions." },
            "limit": { "type": "integer", "description": "Max hits (default 30, max 200); a page may come back shorter, with returned, total, and truncated." }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

pub async fn execute_search_photos<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let query = params
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::invalid_params("Missing 'query' parameter"))?
        .to_string();
    let requested = parse_requested_mode(params.get("mode").and_then(|v| v.as_str()))?;
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| (n as usize).min(MAX_LIMIT))
        .unwrap_or(DEFAULT_LIMIT);
    let volume_filter = params.get("volumeId").and_then(|v| v.as_str()).map(str::to_string);

    // Feature off ⇒ nothing is enriched. Voice it honestly rather than an empty list.
    if !cmdr_index::media_index::gate::is_enabled() {
        return shape(&SearchPhotosResult::ImageIndexingOff {
            note: OFF_NOTE.to_string(),
        });
    }

    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    let model_installed = cmdr_index::media_index::clip::install::state(&data_dir).installed;

    let (mode, degraded_to_ocr) = match resolve_effective_mode(requested, model_installed) {
        ModeResolution::SemanticModelMissing => {
            return shape(&SearchPhotosResult::SemanticModelNotInstalled {
                note: MODEL_MISSING_NOTE.to_string(),
            });
        }
        ModeResolution::Search { mode, degraded_to_ocr } => (mode, degraded_to_ocr),
    };

    let volumes = resolve_search_volumes(volume_filter).await;

    // Per-volume coverage from `media_index`'s own derivation (one source of truth).
    let mut coverage = Vec::new();
    for vid in &volumes {
        if let Ok(state) = crate::commands::media_index::volume_state(app, vid).await {
            coverage.push(derive_coverage(vid, &state));
        }
    }

    // The DB reads + the CLIP text encode run OFF the IPC thread; they answer from
    // `media.db` + the resident cache, so a dead NAS can't hang the tool.
    let (hits, hidden) =
        tauri::async_runtime::spawn_blocking(move || run_search(&data_dir, &volumes, &query, mode, limit))
            .await
            .map_err(|e| ToolError::internal(format!("photo search task panicked: {e}")))?;
    for reason in &hidden {
        note_refusal(ContentTool::SearchPhotos, *reason);
    }

    let note = build_note(degraded_to_ocr, &coverage);
    shape(&shape_ok(mode.wire().to_string(), hits, coverage, note, hidden.len()))
}

/// Build the `Ok` answer, cut to what one tool result may carry: OCR snippets make a
/// 200-hit page far bigger than a prompt budget, so the hits page and the counts say so.
/// Pure, so the paging is unit-tested. Hits stay in rank order, best first.
fn shape_ok(
    mode: String,
    hits: Vec<PhotoHit>,
    coverage: Vec<VolumeCoverage>,
    note: Option<String>,
    protected_hidden: usize,
) -> SearchPhotosResult {
    let fitted = super::fit_to_result_budget(hits);
    SearchPhotosResult::Ok {
        mode,
        returned: fitted.items.len(),
        hits: fitted.items,
        total: fitted.total,
        truncated: fitted.truncated,
        coverage,
        note,
        protected_hidden,
    }
}

/// Serialize a result DTO to the tool's JSON value.
fn shape(result: &SearchPhotosResult) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

/// The volumes to search: the one requested id, or every local/SMB volume (the enrichable
/// kinds — MTP is on-demand, Network is synthetic). An un-enriched volume searches to empty
/// cheaply (the read API short-circuits on a missing `media.db`), so listing extra is safe.
/// Shared with the `image_facts` tool, which resolves its volumes the same way.
pub(super) async fn resolve_search_volumes(volume_filter: Option<String>) -> Vec<String> {
    if let Some(id) = volume_filter {
        return vec![id];
    }
    snapshot_volumes()
        .await
        .into_iter()
        .filter(|v| matches!(v.kind, VolumeKind::Local | VolumeKind::Smb))
        .map(|v| v.id)
        .collect()
}

/// Run the reads for each volume and merge. Encodes the text query once (semantic modes);
/// a missing/unavailable model yields no semantic hits, so `SemanticThenOcr` degrades to the
/// OCR hits it already gathered. Pure DB work — no live `statfs`/`readdir`.
///
/// Answers the hits and each one the gate hid. It hides them BEFORE [`sort_and_cap`], and reads
/// a volume again with a bigger window while hiding left it short, since the read API's own
/// limit would otherwise let a hidden hit eat the answer.
fn run_search(
    data_dir: &Path,
    volumes: &[String],
    query: &str,
    mode: EffectiveMode,
    limit: usize,
) -> (Vec<PhotoHit>, Vec<Protected>) {
    let query_vec = if mode.uses_semantic() {
        cmdr_index::media_index::clip::encode_text_query(query).ok()
    } else {
        None
    };
    let mut all = Vec::new();
    let mut hidden = Vec::new();
    for vid in volumes {
        let index = MediaIndex::open(data_dir, vid);
        let mut fetch = limit;
        loop {
            let semantic = match &query_vec {
                Some(qv) if mode.uses_semantic() => index.search_semantic(qv, fetch),
                _ => Vec::new(),
            };
            let ocr = if mode.uses_ocr() {
                index.search_ocr(query, fetch).unwrap_or_default()
            } else {
                Vec::new()
            };
            let tags = if mode == EffectiveMode::Tag {
                index.images_with_tag(query, 0.0).unwrap_or_default()
            } else {
                Vec::new()
            };
            let hits = merge_hits(vid, mode, semantic, ocr, tags);
            let index_ran_out = hits.len() < fetch;
            let mut hidden_here = Vec::new();
            let kept = hide_protected(hits, &index, &mut hidden_here);
            let short = kept.len() < limit && !hidden_here.is_empty();
            if !short || index_ran_out || fetch >= MAX_HIDING_FETCH {
                all.extend(kept);
                hidden.extend(hidden_here);
                break;
            }
            fetch = fetch.saturating_mul(2).saturating_add(hidden_here.len());
        }
    }
    (sort_and_cap(all, limit), hidden)
}

/// Drops the hits the gate refuses. `classify` is enough for a path: this tool promises a dead
/// NAS can't hang it, so no realpath, and a stored path is what the index knows.
///
/// A text hit carries a snippet of its image's OCR, and a query matching a key's body gets a
/// snippet with no armor in it, so the check reads the whole stored text behind each one. When
/// that read fails, the hit stays and its snippet goes.
fn hide_protected(hits: Vec<PhotoHit>, index: &MediaIndex, hidden: &mut Vec<Protected>) -> Vec<PhotoHit> {
    let mut kept: Vec<PhotoHit> = Vec::with_capacity(hits.len());
    for hit in hits {
        match classify(&hit.path) {
            Some(reason) => hidden.push(reason),
            None => kept.push(hit),
        }
    }
    let with_text: Vec<&str> = kept
        .iter()
        .filter(|hit| hit.match_text.is_some())
        .map(|hit| hit.path.as_str())
        .collect();
    let stored: HashMap<String, Option<String>> = index
        .facts_for_paths(&with_text)
        .unwrap_or_default()
        .into_iter()
        .map(|facts| (facts.path, facts.ocr_text))
        .collect();
    let mut clean = Vec::with_capacity(kept.len());
    for mut hit in kept {
        if hit.match_text.is_some() {
            let Some(text) = stored.get(&hit.path) else {
                hit.match_text = None;
                clean.push(hit);
                continue;
            };
            if let Some(Err(reason)) = text.as_deref().map(refuse_private_key_text) {
                hidden.push(reason);
                continue;
            }
        }
        clean.push(hit);
    }
    clean
}

#[cfg(test)]
#[path = "photos_tests.rs"]
mod tests;
