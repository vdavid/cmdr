//! The `search_photos` tool: photo search by description, in-image text, or tag.
//!
//! Shared by both consumers (agent-spec D49): the in-app Ask Cmdr agent AND external
//! MCP clients dispatch this one registry entry. It only SHAPES the result of the
//! shipped `media_index` read API (`MediaIndex::search_semantic` / `search_ocr` /
//! `images_with_tag`) — the reuse-the-core rule — and reuses `media_index`'s own
//! `volume_state` derivation for coverage honesty rather than deriving a second copy.
//!
//! ## Privacy: what egresses is derived TEXT, not "just metadata"
//!
//! When the in-app agent runs against a cloud provider, the paths AND the in-image OCR
//! snippet / tag this returns are sensitive derived content (a passport scan's OCR
//! snippet IS the passport number). The Ask Cmdr consent gate covers that egress, and
//! its copy names it (`askCmdr.consent.*`, `agent/consent.rs`). What NEVER crosses is
//! image bytes: [`PhotoHit`] is text-only by construction (string/number fields), so the
//! tool structurally can't hand a provider a thumbnail or pixel buffer — pinned by
//! [`tests::photo_hit_is_text_only_no_byte_fields`].

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::{ToolError, ToolResult};
use crate::mcp::resources::volumes::{VolumeKind, snapshot_volumes};
use crate::media_index::commands::MediaIndexVolumeState;
use crate::media_index::read::{MediaIndex, OcrHit, SemanticHit, TagHit};

/// The default hit cap when the caller doesn't specify one, and the hard ceiling on any
/// caller value (an agent never needs a huge grid, and it bounds the payload the LLM reads).
const DEFAULT_LIMIT: usize = 30;
const MAX_LIMIT: usize = 200;

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
        coverage: Vec<VolumeCoverage>,
        /// A coverage caveat the model should relay, when one applies (degraded to OCR
        /// because no model is installed, or a searched volume is still indexing).
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
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
/// `incomplete` when a pass is running, or fewer images are enriched than qualify.
fn derive_coverage(volume: &str, state: &MediaIndexVolumeState) -> VolumeCoverage {
    let incomplete = state.indexing || state.qualifying_count.is_some_and(|q| state.enriched_count < q);
    VolumeCoverage {
        volume: volume.to_string(),
        indexing: state.indexing,
        enriched_count: state.enriched_count,
        qualifying_count: state.qualifying_count,
        incomplete,
    }
}

/// The coverage caveat to relay, if any: OCR degradation leads (it changes what was
/// searched), then a still-indexing note when any searched volume is incomplete.
fn build_note(degraded_to_ocr: bool, coverage: &[VolumeCoverage]) -> Option<String> {
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
            "query": { "type": "string", "description": "What to look for. In semantic mode, a natural description of the scene ('two people on a beach at sunset'); in ocr mode, words expected to appear IN the image (a receipt total, a sign, a passport field); in tag mode, a single object/scene label ('dog', 'sky')." },
            "volumeId": { "type": "string", "description": "Restrict to one volume id (see list_volumes / cmdr://state). Omit to search every local and SMB volume that's indexed." },
            "mode": { "type": "string", "enum": ["semantic", "ocr", "tag"], "description": "How to match. semantic: by visual description (needs the on-device model). ocr: by text recognized inside the image. tag: by a Vision object/scene tag. Omit to combine semantic + ocr (semantic leads), which is best for most 'find the photo of…' questions." },
            "limit": { "type": "integer", "description": "Max hits to return (default 30, capped at 200)." }
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
    if !crate::media_index::gate::is_enabled() {
        return shape(&SearchPhotosResult::ImageIndexingOff {
            note: OFF_NOTE.to_string(),
        });
    }

    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    let model_installed = crate::media_index::clip::install::is_installed(&data_dir);

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
        if let Ok(state) = crate::media_index::commands::volume_state(app, vid).await {
            coverage.push(derive_coverage(vid, &state));
        }
    }

    // The DB reads + the CLIP text encode run OFF the IPC thread; they answer from
    // `media.db` + the resident cache, so a dead NAS can't hang the tool.
    let hits = tauri::async_runtime::spawn_blocking(move || run_search(&data_dir, &volumes, &query, mode, limit))
        .await
        .map_err(|e| ToolError::internal(format!("photo search task panicked: {e}")))?;

    let note = build_note(degraded_to_ocr, &coverage);
    shape(&SearchPhotosResult::Ok {
        mode: mode.wire().to_string(),
        hits,
        coverage,
        note,
    })
}

/// Serialize a result DTO to the tool's JSON value.
fn shape(result: &SearchPhotosResult) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

/// The volumes to search: the one requested id, or every local/SMB volume (the enrichable
/// kinds — MTP is on-demand, Network is synthetic). An un-enriched volume searches to empty
/// cheaply (the read API short-circuits on a missing `media.db`), so listing extra is safe.
async fn resolve_search_volumes(volume_filter: Option<String>) -> Vec<String> {
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
fn run_search(data_dir: &Path, volumes: &[String], query: &str, mode: EffectiveMode, limit: usize) -> Vec<PhotoHit> {
    let query_vec = if mode.uses_semantic() {
        crate::media_index::clip::encode_text_query(query).ok()
    } else {
        None
    };
    let mut all = Vec::new();
    for vid in volumes {
        let index = MediaIndex::open(data_dir, vid);
        let semantic = match &query_vec {
            Some(qv) if mode.uses_semantic() => index.search_semantic(qv, limit),
            _ => Vec::new(),
        };
        let ocr = if mode.uses_ocr() {
            index.search_ocr(query, limit).unwrap_or_default()
        } else {
            Vec::new()
        };
        let tags = if mode == EffectiveMode::Tag {
            index.images_with_tag(query, 0.0).unwrap_or_default()
        } else {
            Vec::new()
        };
        all.extend(merge_hits(vid, mode, semantic, ocr, tags));
    }
    sort_and_cap(all, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic(path: &str, score: f32) -> SemanticHit {
        SemanticHit {
            path: path.to_string(),
            score,
        }
    }
    fn ocr(path: &str, snippet: &str) -> OcrHit {
        OcrHit {
            path: path.to_string(),
            snippet: snippet.to_string(),
        }
    }
    fn tag(path: &str, score: f32) -> TagHit {
        TagHit {
            path: path.to_string(),
            score,
        }
    }

    #[test]
    fn parse_requested_mode_maps_known_and_rejects_unknown() {
        assert_eq!(parse_requested_mode(None).unwrap(), RequestedMode::Auto);
        assert_eq!(parse_requested_mode(Some("semantic")).unwrap(), RequestedMode::Semantic);
        assert_eq!(parse_requested_mode(Some("ocr")).unwrap(), RequestedMode::Ocr);
        assert_eq!(parse_requested_mode(Some("tag")).unwrap(), RequestedMode::Tag);
        assert!(parse_requested_mode(Some("faces")).is_err());
    }

    #[test]
    fn resolve_effective_mode_mirrors_ui_and_is_honest_about_the_model() {
        // Auto with a model composes semantic + OCR; without a model it degrades to OCR.
        assert_eq!(
            resolve_effective_mode(RequestedMode::Auto, true),
            ModeResolution::Search {
                mode: EffectiveMode::SemanticThenOcr,
                degraded_to_ocr: false
            }
        );
        assert_eq!(
            resolve_effective_mode(RequestedMode::Auto, false),
            ModeResolution::Search {
                mode: EffectiveMode::OcrOnly,
                degraded_to_ocr: true
            }
        );
        // An EXPLICIT semantic ask with no model is an honest status, not a silent OCR swap.
        assert_eq!(
            resolve_effective_mode(RequestedMode::Semantic, false),
            ModeResolution::SemanticModelMissing
        );
        assert_eq!(
            resolve_effective_mode(RequestedMode::Semantic, true),
            ModeResolution::Search {
                mode: EffectiveMode::SemanticOnly,
                degraded_to_ocr: false
            }
        );
        // OCR and tag never depend on the model.
        assert_eq!(
            resolve_effective_mode(RequestedMode::Ocr, false),
            ModeResolution::Search {
                mode: EffectiveMode::OcrOnly,
                degraded_to_ocr: false
            }
        );
        assert_eq!(
            resolve_effective_mode(RequestedMode::Tag, false),
            ModeResolution::Search {
                mode: EffectiveMode::Tag,
                degraded_to_ocr: false
            }
        );
    }

    #[test]
    fn merge_hits_leads_with_semantic_then_dedups_ocr_by_path() {
        // A path matched by BOTH semantic and OCR appears once, as the description hit.
        let hits = merge_hits(
            "root",
            EffectiveMode::SemanticThenOcr,
            vec![semantic("/a.jpg", 0.9), semantic("/b.jpg", 0.8)],
            vec![ocr("/a.jpg", "[a]"), ocr("/c.jpg", "[c]")],
            vec![],
        );
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, vec!["/a.jpg", "/b.jpg", "/c.jpg"]);
        assert_eq!(hits[0].match_kind, "description");
        assert_eq!(hits[0].score, Some(0.9));
        assert_eq!(hits[0].match_text, None);
        // The deduped /a.jpg stays a description hit (semantic leads), not re-added as text.
        assert_eq!(hits.iter().filter(|h| h.path == "/a.jpg").count(), 1);
        // /c.jpg is an OCR text hit carrying its snippet.
        let c = hits.iter().find(|h| h.path == "/c.jpg").unwrap();
        assert_eq!(c.match_kind, "text");
        assert_eq!(c.match_text.as_deref(), Some("[c]"));
        assert_eq!(c.score, None);
    }

    #[test]
    fn merge_hits_ocr_only_and_tag_only_carry_the_right_kind() {
        let ocr_hits = merge_hits(
            "root",
            EffectiveMode::OcrOnly,
            vec![],
            vec![ocr("/x.jpg", "[x]")],
            vec![],
        );
        assert_eq!(ocr_hits.len(), 1);
        assert_eq!(ocr_hits[0].match_kind, "text");
        assert_eq!(ocr_hits[0].match_text.as_deref(), Some("[x]"));

        let tag_hits = merge_hits("root", EffectiveMode::Tag, vec![], vec![], vec![tag("/dog.jpg", 0.77)]);
        assert_eq!(tag_hits.len(), 1);
        assert_eq!(tag_hits[0].match_kind, "tag");
        assert_eq!(tag_hits[0].score, Some(0.77));
        assert_eq!(tag_hits[0].match_text, None);

        // OcrOnly never emits semantic hits even when some are passed.
        let no_semantic = merge_hits(
            "root",
            EffectiveMode::OcrOnly,
            vec![semantic("/s.jpg", 0.5)],
            vec![],
            vec![],
        );
        assert!(no_semantic.is_empty());
    }

    #[test]
    fn sort_and_cap_orders_by_score_then_caps() {
        let hits = vec![
            PhotoHit {
                path: "/low.jpg".into(),
                volume: "root".into(),
                match_kind: "description".into(),
                score: Some(0.3),
                match_text: None,
            },
            PhotoHit {
                path: "/text.jpg".into(),
                volume: "root".into(),
                match_kind: "text".into(),
                score: None,
                match_text: Some("[t]".into()),
            },
            PhotoHit {
                path: "/high.jpg".into(),
                volume: "root".into(),
                match_kind: "description".into(),
                score: Some(0.95),
                match_text: None,
            },
        ];
        let capped = sort_and_cap(hits, 2);
        assert_eq!(capped.len(), 2);
        // Highest score first; the scoreless text hit sinks below the scored ones.
        assert_eq!(capped[0].path, "/high.jpg");
        assert_eq!(capped[1].path, "/low.jpg");
    }

    fn state(indexing: bool, enriched: u64, qualifying: Option<u64>) -> MediaIndexVolumeState {
        MediaIndexVolumeState {
            enabled: true,
            indexing,
            enriched_count: enriched,
            qualifying_count: qualifying,
            network_opt_in: false,
            always_indexed: false,
            paused: false,
            waiting_for_importance: false,
            covered_qualifying_count: None,
            kept_count: None,
        }
    }

    #[test]
    fn derive_coverage_flags_incomplete_when_indexing_or_short() {
        // A running pass ⇒ incomplete regardless of counts.
        assert!(derive_coverage("root", &state(true, 10, Some(10))).incomplete);
        // Fewer enriched than qualify ⇒ incomplete.
        assert!(derive_coverage("root", &state(false, 5, Some(20))).incomplete);
        // Done and idle ⇒ complete.
        assert!(!derive_coverage("root", &state(false, 20, Some(20))).incomplete);
        // Unknown denominator, idle ⇒ not flagged incomplete off a missing count.
        assert!(!derive_coverage("root", &state(false, 20, None)).incomplete);
    }

    #[test]
    fn build_note_leads_with_degradation_then_indexing() {
        let complete = vec![VolumeCoverage {
            volume: "root".into(),
            indexing: false,
            enriched_count: 10,
            qualifying_count: Some(10),
            incomplete: false,
        }];
        assert!(build_note(false, &complete).is_none());

        let incomplete = vec![VolumeCoverage {
            volume: "root".into(),
            indexing: true,
            enriched_count: 3,
            qualifying_count: Some(10),
            incomplete: true,
        }];
        assert!(build_note(false, &incomplete).unwrap().contains("still indexing"));
        // Degradation wins the note slot even when a volume is also incomplete.
        assert!(build_note(true, &incomplete).unwrap().contains("in-image text only"));
    }

    #[test]
    fn honest_statuses_serialize_with_a_typed_status_tag() {
        let off = serde_json::to_value(SearchPhotosResult::ImageIndexingOff { note: "n".into() }).unwrap();
        assert_eq!(off["status"], "imageIndexingOff");

        let missing = serde_json::to_value(SearchPhotosResult::SemanticModelNotInstalled { note: "n".into() }).unwrap();
        assert_eq!(missing["status"], "semanticModelNotInstalled");

        let ok = serde_json::to_value(SearchPhotosResult::Ok {
            mode: "semantic+ocr".into(),
            hits: vec![],
            coverage: vec![],
            note: None,
        })
        .unwrap();
        assert_eq!(ok["status"], "ok");
        // An absent note doesn't clutter the payload.
        assert!(ok.get("note").is_none());
    }

    /// The load-bearing privacy property: a hit is TEXT-ONLY, so the tool structurally
    /// can't hand a provider image bytes or a thumbnail. Every serialized value is a
    /// string or number (never a JSON array, which could smuggle a byte vector), and the
    /// keys are exactly the allowed text/number fields.
    #[test]
    fn photo_hit_is_text_only_no_byte_fields() {
        let hit = PhotoHit {
            path: "/passport.jpg".into(),
            volume: "root".into(),
            match_kind: "text".into(),
            score: Some(0.5),
            match_text: Some("[passport]".into()),
        };
        let json = serde_json::to_value(&hit).unwrap();
        let obj = json.as_object().expect("a JSON object");
        let allowed = ["path", "volume", "matchKind", "score", "matchText"];
        for key in obj.keys() {
            assert!(allowed.contains(&key.as_str()), "unexpected field '{key}' on PhotoHit");
        }
        for (key, value) in obj {
            assert!(
                value.is_string() || value.is_number(),
                "field '{key}' must be text or a number (no bytes), got {value}"
            );
        }
    }
}
