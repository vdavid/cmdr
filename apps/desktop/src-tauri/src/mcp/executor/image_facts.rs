//! The `image_facts` tool: what the media index knows about images the caller ALREADY has.
//!
//! The LOOKUP direction, and the mirror of [`super::photos`]'s `search_photos`: that one
//! answers "which files match this query", this one answers "what's in these files". A
//! bulk-rename flow already has the paths (the user navigated to the folder) and needs to
//! know what each image contains before it can propose a name. It only SHAPES the shipped
//! [`MediaIndex::facts_for_paths`] read API (the reuse-the-core rule) and reuses
//! `photos.rs`'s coverage derivation, so honesty can't drift between the two tools.
//!
//! ## Privacy: the FULL recognized text egresses, not a snippet
//!
//! `search_photos` returns a highlighted snippet around a match; this returns the whole
//! stored OCR text, because a model naming a file has to read all of it. That's the most
//! sensitive thing either tool emits (a passport scan's OCR text IS the passport number).
//! The Ask Cmdr consent gate (`agent/consent.rs`, enforced on every send) covers the
//! egress and its copy names it: "the text Cmdr recognized inside … photos and their tags".
//! What NEVER crosses is image bytes: [`FileFacts`] is text-only by construction, pinned by
//! [`tests::file_facts_is_text_only_no_byte_fields`].

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::photos::{VolumeCoverage, build_note, derive_coverage, resolve_search_volumes};
use super::{ToolError, ToolResult};
use crate::media_index::read::{ImageFacts, ImageTag, MediaIndex};

/// The most paths one call accepts. Over this is a hard `INVALID_PARAMS` rather than a
/// silent truncation: a caller that asked about 500 files must not believe it got 200
/// answers for all of them. It also bounds the response the calling model has to read.
const MAX_PATHS: usize = 200;

/// The per-file cap on returned OCR text, in characters. Enough to name a file by its
/// contents; short enough that 200 dense receipts can't blow the model's context. A cut is
/// always flagged (`textTruncated`), never silent.
const MAX_TEXT_CHARS: usize = 2_000;

/// Honest note when image indexing is off entirely.
const OFF_NOTE: &str = "Image indexing is off, so there's nothing stored about these images. The user can turn it on under Settings › AI › Image search.";

// ── Result DTOs ─────────────────────────────────────────────────────────────

/// Whether the index has anything for a path. Typed, so a caller branches on a variant
/// rather than sniffing for an absent field (`no-string-matching`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FactsState {
    /// Enrichment ran for this path. `text`/`tags` may still be empty (nothing found).
    Indexed,
    /// No enrichment row on any searched volume: not indexed yet, excluded, or not an image.
    NotIndexed,
}

/// One stored tag: label + confidence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FactTag {
    pub label: String,
    pub score: f32,
}

/// What the index knows about one requested file. TEXT-ONLY by construction: strings,
/// flags, and tag label/score pairs, so the tool can't hand a provider image bytes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFacts {
    /// The path exactly as requested (after `~` expansion), so the caller can join back.
    pub path: String,
    pub state: FactsState,
    /// The volume whose index answered. Absent when nothing was found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// The text recognized inside the image, up to [`MAX_TEXT_CHARS`]. Absent when the
    /// image has none. This IS image-derived user content — see the module's privacy note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The Vision scene/object tags, highest confidence first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<FactTag>,
    /// `true` when `text` was cut at the cap, so the model knows it's reading a prefix.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub text_truncated: bool,
}

/// The tool result. A typed status the model relays honestly — never a bare list of
/// "not indexed" rows that hides "image indexing is off".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ImageFactsResult {
    /// Image indexing is off, so nothing is stored for any image.
    ImageIndexingOff { note: String },
    /// A normal answer: one row per requested path, plus per-volume coverage.
    Ok {
        facts: Vec<FileFacts>,
        coverage: Vec<VolumeCoverage>,
        /// A coverage caveat to relay, when a searched volume is still indexing.
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

// ── Param parsing + shaping (pure) ────────────────────────────────────────────

/// Parse the required `paths` array: absolute paths, `~` expanded (agents routinely send
/// `~/Pictures/…`, and a literal `~` would never match a stored path). Empty, non-array,
/// non-string, blank-only, and over-cap inputs are all hard param errors.
fn parse_paths(params: &Value) -> Result<Vec<String>, ToolError> {
    let raw = params
        .get("paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ToolError::invalid_params("Missing 'paths' parameter (an array of file paths)"))?;
    if raw.is_empty() {
        return Err(ToolError::invalid_params("'paths' must list at least one file"));
    }
    if raw.len() > MAX_PATHS {
        return Err(ToolError::invalid_params(format!(
            "'paths' holds {} files, more than the {MAX_PATHS} this returns at once. Ask about them in batches.",
            raw.len()
        )));
    }
    raw.iter()
        .map(|v| {
            v.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(super::expand_user_path)
                .ok_or_else(|| ToolError::invalid_params("Every entry in 'paths' must be a non-empty file path"))
        })
        .collect()
}

/// One unresolved row per requested path. Every path gets an answer, so a caller can join
/// results back to its own list without guessing which ones went missing.
fn not_indexed_slots(paths: &[String]) -> Vec<FileFacts> {
    paths
        .iter()
        .map(|path| FileFacts {
            path: path.clone(),
            state: FactsState::NotIndexed,
            volume: None,
            text: None,
            tags: Vec::new(),
            text_truncated: false,
        })
        .collect()
}

/// Cut `text` to `cap` CHARACTERS (never mid-codepoint), reporting whether it was cut.
fn truncate_text(text: String, cap: usize) -> (String, bool) {
    if text.chars().count() <= cap {
        return (text, false);
    }
    (text.chars().take(cap).collect(), true)
}

/// Fold one volume's read into the answer rows. Positional: `facts` is
/// `facts_for_paths`'s per-request-path output, so it lines up with `out`; a short or
/// empty vec (a volume whose read errored) leaves the remaining slots untouched. Only
/// UNRESOLVED slots are filled, so the first volume that knows a path wins and a later
/// volume can't overwrite it.
fn merge_volume(out: &mut [FileFacts], volume: &str, facts: Vec<ImageFacts>, text_cap: usize) {
    for (slot, found) in out.iter_mut().zip(facts) {
        if slot.state != FactsState::NotIndexed || !found.indexed {
            continue;
        }
        let (text, text_truncated) = match found.ocr_text {
            Some(t) => {
                let (t, cut) = truncate_text(t, text_cap);
                (Some(t), cut)
            }
            None => (None, false),
        };
        slot.state = FactsState::Indexed;
        slot.volume = Some(volume.to_string());
        slot.text = text;
        slot.text_truncated = text_truncated;
        slot.tags = found
            .tags
            .into_iter()
            .map(|ImageTag { label, score }| FactTag { label, score })
            .collect();
    }
}

// ── Schema + handler ──────────────────────────────────────────────────────────

pub fn image_facts_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "The absolute paths of the images to look up (at most 200 per call). Use this when you already know the files, for example everything in the folder the user is looking at."
            },
            "volumeId": { "type": "string", "description": "Restrict the lookup to one volume id (see list_volumes / cmdr://state). Omit to check every local and SMB volume that's indexed." }
        },
        "required": ["paths"],
        "additionalProperties": false
    })
}

pub async fn execute_image_facts<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let paths = parse_paths(params)?;
    let volume_filter = params.get("volumeId").and_then(|v| v.as_str()).map(str::to_string);

    // Feature off ⇒ nothing is stored. Voice it honestly rather than a wall of
    // "not indexed" rows the model would read as "these images are empty".
    if !crate::media_index::gate::is_enabled() {
        return shape(&ImageFactsResult::ImageIndexingOff {
            note: OFF_NOTE.to_string(),
        });
    }

    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    let volumes = resolve_search_volumes(volume_filter).await;

    // Per-volume coverage from `media_index`'s own derivation (one source of truth).
    let mut coverage = Vec::new();
    for vid in &volumes {
        if let Ok(state) = crate::media_index::commands::volume_state(app, vid).await {
            coverage.push(derive_coverage(vid, &state));
        }
    }

    // The DB reads run OFF the IPC thread; they answer from `media.db` alone, so a dead
    // NAS can't hang the tool.
    let facts = tauri::async_runtime::spawn_blocking(move || run_lookup(&data_dir, &volumes, &paths))
        .await
        .map_err(|e| ToolError::internal(format!("image facts task panicked: {e}")))?;

    let note = build_note(false, &coverage);
    shape(&ImageFactsResult::Ok { facts, coverage, note })
}

/// Serialize a result DTO to the tool's JSON value.
fn shape(result: &ImageFactsResult) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

/// Read each volume's media index and fold the answers together. A path lives on exactly
/// one volume, so the first volume that has a row for it wins; an un-enriched volume
/// short-circuits on its missing `media.db`, making the extra volumes cheap. Pure DB work
/// — no live `statfs`/`readdir`.
fn run_lookup(data_dir: &Path, volumes: &[String], paths: &[String]) -> Vec<FileFacts> {
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut out = not_indexed_slots(paths);
    for vid in volumes {
        if out.iter().all(|f| f.state == FactsState::Indexed) {
            break;
        }
        let facts = MediaIndex::open(data_dir, vid)
            .facts_for_paths(&refs)
            .unwrap_or_default();
        merge_volume(&mut out, vid, facts, MAX_TEXT_CHARS);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_paths_requires_a_non_empty_bounded_array() {
        assert!(parse_paths(&json!({})).is_err());
        assert!(parse_paths(&json!({ "paths": "/a.jpg" })).is_err());
        assert!(parse_paths(&json!({ "paths": [] })).is_err());
        assert!(parse_paths(&json!({ "paths": ["  "] })).is_err());
        assert!(parse_paths(&json!({ "paths": [1, 2] })).is_err());

        let ok = parse_paths(&json!({ "paths": ["/a.jpg", " /b.jpg "] })).expect("valid");
        assert_eq!(ok, vec!["/a.jpg".to_string(), "/b.jpg".to_string()]);

        // Over the cap is a hard param error, not a silent truncation: the caller must
        // know it didn't get facts for everything it asked about.
        let too_many: Vec<String> = (0..=MAX_PATHS).map(|i| format!("/img-{i}.jpg")).collect();
        assert!(parse_paths(&json!({ "paths": too_many })).is_err());
    }

    #[test]
    fn parse_paths_expands_a_leading_tilde() {
        let paths = parse_paths(&json!({ "paths": ["~/Pictures/a.jpg"] })).expect("valid");
        assert!(!paths[0].starts_with('~'), "a literal ~ never matches a stored path");
    }

    #[test]
    fn truncate_text_flags_the_cut_and_respects_char_boundaries() {
        assert_eq!(truncate_text("hello".to_string(), 10), ("hello".to_string(), false));
        let (cut, truncated) = truncate_text("åäöåäö".to_string(), 3);
        assert!(truncated);
        assert_eq!(cut, "åäö", "cuts on chars, never mid-codepoint");
    }

    fn facts(path: &str, indexed: bool, text: Option<&str>, tags: Vec<(&str, f32)>) -> ImageFacts {
        ImageFacts {
            path: path.to_string(),
            indexed,
            ocr_text: text.map(str::to_string),
            tags: tags
                .into_iter()
                .map(|(label, score)| ImageTag {
                    label: label.to_string(),
                    score,
                })
                .collect(),
        }
    }

    #[test]
    fn merge_volume_fills_only_unresolved_slots_and_keeps_the_first_volume_that_knows() {
        let mut out = not_indexed_slots(&["/a.jpg".to_string(), "/b.jpg".to_string()]);
        merge_volume(
            &mut out,
            "root",
            vec![
                facts("/a.jpg", true, Some("alpha"), vec![("paper", 0.7)]),
                facts("/b.jpg", false, None, vec![]),
            ],
            100,
        );
        assert_eq!(out[0].state, FactsState::Indexed);
        assert_eq!(out[0].volume.as_deref(), Some("root"));
        assert_eq!(out[0].text.as_deref(), Some("alpha"));
        assert_eq!(out[0].tags[0].label, "paper");
        assert_eq!(out[1].state, FactsState::NotIndexed);

        // A second volume fills the still-unresolved slot and never overwrites a resolved one.
        merge_volume(
            &mut out,
            "usb",
            vec![
                facts("/a.jpg", true, Some("SHOULD NOT WIN"), vec![]),
                facts("/b.jpg", true, None, vec![("sky", 0.9)]),
            ],
            100,
        );
        assert_eq!(out[0].text.as_deref(), Some("alpha"), "first volume that knows wins");
        assert_eq!(out[0].volume.as_deref(), Some("root"));
        assert_eq!(out[1].state, FactsState::Indexed);
        assert_eq!(out[1].volume.as_deref(), Some("usb"));
        // Indexed with no text is NOT the same as never indexed: the caller must be able
        // to say "there's nothing in it" rather than "ask me again later".
        assert_eq!(out[1].text, None);
        assert_eq!(out[1].tags[0].label, "sky");
    }

    #[test]
    fn merge_volume_truncates_long_text_and_flags_it() {
        let mut out = not_indexed_slots(&["/a.jpg".to_string()]);
        merge_volume(&mut out, "root", vec![facts("/a.jpg", true, Some("abcdef"), vec![])], 3);
        assert_eq!(out[0].text.as_deref(), Some("abc"));
        assert!(out[0].text_truncated, "a cut must be visible to the model");
    }

    #[test]
    fn merge_volume_tolerates_a_short_read() {
        // A per-volume read that errored yields an empty vec; slots must survive untouched.
        let mut out = not_indexed_slots(&["/a.jpg".to_string(), "/b.jpg".to_string()]);
        merge_volume(&mut out, "root", vec![], 100);
        assert!(out.iter().all(|f| f.state == FactsState::NotIndexed));
    }

    #[test]
    fn honest_statuses_serialize_with_a_typed_status_tag() {
        let off = serde_json::to_value(ImageFactsResult::ImageIndexingOff { note: "n".into() }).unwrap();
        assert_eq!(off["status"], "imageIndexingOff");

        let ok = serde_json::to_value(ImageFactsResult::Ok {
            facts: vec![],
            coverage: vec![],
            note: None,
        })
        .unwrap();
        assert_eq!(ok["status"], "ok");
        assert!(ok.get("note").is_none(), "an absent note doesn't clutter the payload");
    }

    /// The privacy property, mirroring `photos.rs`: a result row is TEXT-ONLY by
    /// construction, so the tool structurally can't hand a provider image bytes or a
    /// thumbnail. Every field is a string/bool, and the one array (`tags`) holds only a
    /// label and a score — no place for a byte vector to hide.
    #[test]
    fn file_facts_is_text_only_no_byte_fields() {
        let mut out = not_indexed_slots(&["/passport.jpg".to_string()]);
        merge_volume(
            &mut out,
            "root",
            vec![facts("/passport.jpg", true, Some("P<SWE"), vec![("document", 0.9)])],
            2,
        );
        let json = serde_json::to_value(&out[0]).unwrap();
        let obj = json.as_object().expect("a JSON object");
        let allowed = ["path", "state", "volume", "text", "tags", "textTruncated"];
        for key in obj.keys() {
            assert!(allowed.contains(&key.as_str()), "unexpected field '{key}' on FileFacts");
        }
        for (key, value) in obj {
            if key == "tags" {
                for tag in value.as_array().expect("tags is an array") {
                    let tag = tag.as_object().expect("a tag object");
                    assert_eq!(tag.len(), 2);
                    assert!(tag["label"].is_string());
                    assert!(tag["score"].is_number());
                }
                continue;
            }
            assert!(
                value.is_string() || value.is_boolean(),
                "field '{key}' must be text or a flag (no bytes), got {value}"
            );
        }
    }
}
