//! The `tag` tool: set macOS Finder color tags on files.
//!
//! Thin adapter over `file_system::tags` (`toggle_color` / `set_tags`) — the same
//! primitives the context-menu tag toggle uses. It resolves target paths off the
//! pane state (names / selection / cursor, via `resolve_pane_target_paths`), then
//! patches the cached listing so the panes re-render the colored dots (reusing
//! `Listings::apply_tags`, the enrich path). Gate `Always`: silent metadata
//! mutation on user files, with no confirmation dialog to piggyback on.
//!
//! macOS-only: Finder tags don't exist elsewhere, so off macOS it returns a clean
//! not-supported error.

use serde_json::Value;
#[cfg(target_os = "macos")]
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{ToolError, ToolResult};

pub async fn execute_tag<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, params);
        Err(ToolError::internal("Finder tags are only supported on macOS"))
    }
    #[cfg(target_os = "macos")]
    {
        execute_tag_macos(app, params).await
    }
}

#[cfg(target_os = "macos")]
async fn execute_tag_macos<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;
    if !["set", "toggle", "clear"].contains(&action) {
        return Err(ToolError::invalid_params(format!(
            "action must be 'set', 'toggle', or 'clear' (got '{action}')"
        )));
    }

    let colors = parse_colors(params)?;
    if action != "clear" && colors.is_empty() {
        return Err(ToolError::invalid_params(format!(
            "'{action}' requires a non-empty 'colors' array (for example [\"red\", \"blue\"])"
        )));
    }

    let names = params.get("names").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>()
    });

    // Resolution reads PaneStateStore, so flush the FE's pending push first (the
    // `move_cursor` / `select` freshness, without moving the cursor): otherwise a
    // tag right after a bare `nav` could resolve a same-named file from the pane's
    // previous directory.
    let (pane, _stale) = super::target_pane_state(app, params)?;
    super::flush_pane_state(app, &pane).await?;
    let (_pane, state) = super::target_pane_state(app, params)?;
    let paths = super::resolve_pane_target_paths(&state, names.as_deref())?;

    let outcome = apply_tag_action(action, &paths, &colors)?;
    refresh_listing_tags(&state, outcome.updates);

    // Flush the pane's MCP state so the `[tags:…]` marker shows in cmdr://state
    // promptly instead of ~2 s later (the debounced sync). `refresh_listing_tags`
    // patched the backend the listing cache synchronously, and the FE's
    // `syncPaneStateToMcp` re-reads that cache via `getFileAt` — so the forced push
    // carries the new tags without racing the 50 ms-coalesced `directory-diff`.
    // Best-effort: the tags already landed on disk and in the cache, so a flush
    // timeout just delays the marker to the next push rather than failing the tool.
    let _ = super::flush_pane_state(app, &pane).await;

    let changed = paths.len() - outcome.skipped.len();
    Ok(json!(format!(
        "OK: {} tags on {} in the {pane} pane.{}",
        action_verb(action),
        crate::pluralize::pluralize(changed as u64, "item"),
        skipped_note(&outcome.skipped)
    )))
}

/// How many skipped paths the result names before summarizing the rest, so a big
/// selection of unreadable files can't flood the caller's context.
#[cfg(target_os = "macos")]
const MAX_SKIPS_NAMED: usize = 10;

/// The result's account of files left alone because their existing tags couldn't be
/// read or decoded. Empty when nothing was skipped.
#[cfg(target_os = "macos")]
fn skipped_note(skipped: &[crate::file_system::tags::TagSkip]) -> String {
    if skipped.is_empty() {
        return String::new();
    }
    let named: Vec<String> = skipped
        .iter()
        .take(MAX_SKIPS_NAMED)
        .map(|s| format!("{} ({})", s.path, s.reason))
        .collect();
    let more = skipped.len().saturating_sub(MAX_SKIPS_NAMED);
    let tail = if more > 0 {
        format!("; and {more} more")
    } else {
        String::new()
    };
    format!(
        " Skipped {} whose existing tags couldn't be read, so their tags are unchanged: {}{tail}.",
        crate::pluralize::pluralize(skipped.len() as u64, "item"),
        named.join("; ")
    )
}

/// Parse the `colors` array (color-name strings) into system color indices. An
/// absent `colors` yields an empty vec (valid for `clear`); an unknown name is an
/// honest error naming the accepted set.
#[cfg(target_os = "macos")]
fn parse_colors(params: &Value) -> Result<Vec<u8>, ToolError> {
    let Some(arr) = params.get("colors").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let name = v
            .as_str()
            .ok_or_else(|| ToolError::invalid_params("'colors' must be an array of color-name strings"))?;
        out.push(color_index(name).ok_or_else(|| {
            ToolError::invalid_params(format!(
                "unknown color '{name}'; use one of red, orange, yellow, green, blue, purple, gray"
            ))
        })?);
    }
    Ok(out)
}

/// The seven Finder color names → the system color index (1..=7). `grey` is an
/// accepted alias for `gray`. Case-insensitive.
#[cfg(target_os = "macos")]
fn color_index(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "gray" | "grey" => 1,
        "green" => 2,
        "purple" => 3,
        "blue" => 4,
        "yellow" => 5,
        "red" => 6,
        "orange" => 7,
        _ => return None,
    })
}

/// Applies the tag mutation and returns the resulting per-path tag sets so the
/// caller can patch the listing cache, plus the files left alone. `set` preserves
/// colorless (custom-named) tags and any existing colored tag whose color is
/// requested; `toggle` uses Finder's all-have→remove rule per color; `clear`
/// removes every tag.
///
/// `set` and `toggle` both MERGE with what's on disk, so a file whose existing tags
/// can't be read or fully decoded is skipped and reported, never treated as
/// untagged (that would drop its custom tags). `clear` reads nothing to keep: the
/// caller asked for every tag gone, so it writes regardless.
#[cfg(target_os = "macos")]
fn apply_tag_action(
    action: &str,
    paths: &[String],
    colors: &[u8],
) -> Result<crate::file_system::tags::TagToggleOutcome, ToolError> {
    use crate::file_system::tags::{TagSkip, TagToggleOutcome, read_tags_strict, set_tags, toggle_color};
    use std::path::Path;

    match action {
        "clear" => {
            let mut updates = Vec::with_capacity(paths.len());
            for p in paths {
                set_tags(Path::new(p), &[])
                    .map_err(|e| ToolError::internal(format!("Couldn't clear tags on {p}: {e}")))?;
                updates.push((p.clone(), Vec::new()));
            }
            Ok(TagToggleOutcome {
                updates,
                skipped: Vec::new(),
            })
        }
        "toggle" => {
            // Toggle each requested color across all paths. Each `toggle_color`
            // reads current on-disk state, so the last color's result reflects
            // every prior toggle — it's the refresh snapshot. `colors` is
            // validated non-empty for toggle, so `last` is always assigned. An
            // unreadable file is skipped by every pass, so the first pass's
            // skips are the whole list.
            let mut last = TagToggleOutcome::default();
            let mut skipped: Option<Vec<TagSkip>> = None;
            for &color in colors {
                last =
                    toggle_color(paths, color).map_err(|e| ToolError::internal(format!("Couldn't toggle tag: {e}")))?;
                skipped.get_or_insert(std::mem::take(&mut last.skipped));
            }
            last.skipped = skipped.unwrap_or_default();
            Ok(last)
        }
        "set" => {
            let mut outcome = TagToggleOutcome::default();
            for p in paths {
                let current = match read_tags_strict(Path::new(p)) {
                    Ok(tags) => tags.unwrap_or_default(),
                    Err(reason) => {
                        outcome.skipped.push(TagSkip {
                            path: p.clone(),
                            reason,
                        });
                        continue;
                    }
                };
                let desired = desired_tags_for_set(&current, colors);
                // Skip files already in the target state so we don't churn mtimes.
                if desired != current {
                    set_tags(Path::new(p), &desired)
                        .map_err(|e| ToolError::internal(format!("Couldn't set tags on {p}: {e}")))?;
                }
                outcome.updates.push((p.clone(), desired));
            }
            Ok(outcome)
        }
        _ => Err(ToolError::internal("unreachable: action validated by caller")),
    }
}

/// The desired tag set for `set`: keep every colorless (custom-named) tag, keep an
/// existing colored tag whose color is requested (preserving a custom name), drop
/// colored tags whose color isn't requested, and add the canonical system tag for
/// a requested color the file lacks. Pure, so it's unit-testable.
#[cfg(target_os = "macos")]
fn desired_tags_for_set(current: &[cmdr_fs::entry::TagRef], colors: &[u8]) -> Vec<cmdr_fs::entry::TagRef> {
    use cmdr_fs::entry::TagRef;
    let mut out: Vec<TagRef> = current.iter().filter(|t| t.color == 0).cloned().collect();
    for &color in colors {
        if out.iter().any(|t| t.color == color) {
            continue; // deduped requested colors
        }
        if let Some(existing) = current.iter().find(|t| t.color == color) {
            out.push(existing.clone());
        } else if let Some(name) = crate::file_system::tags::system_color_name(color) {
            out.push(TagRef {
                name: name.to_string(),
                color,
            });
        }
    }
    out
}

/// Patch the resulting tags into the focused pane's cached listing so the dots
/// re-render immediately, reusing the same `Listings::apply_tags` path the
/// context-menu toggle and `enrich_tags` use. No matching listing (a rare
/// path/volume mismatch) simply skips the in-place refresh — the tags still
/// landed on disk and the dots update on the next visible-range enrich.
#[cfg(target_os = "macos")]
fn refresh_listing_tags(
    state: &crate::mcp::pane_state::PaneState,
    updates: Vec<(String, Vec<cmdr_fs::entry::TagRef>)>,
) {
    if updates.is_empty() {
        return;
    }
    let volume_id = state.volume_id.as_deref().unwrap_or("root");
    let listings = crate::listing_host::listings();
    // The youngest listing showing the pane's folder, as the pane's own.
    let showing = listings.listings_for_path_on_volume(volume_id, std::path::Path::new(&state.path));
    let listing = listings
        .summaries()
        .into_iter()
        .find(|summary| showing.contains(&summary.listing_id));
    if let Some(listing) = listing {
        listings.apply_tags(&listing.listing_id, updates);
    }
}

#[cfg(target_os = "macos")]
fn action_verb(action: &str) -> &'static str {
    match action {
        "set" => "Set",
        "toggle" => "Toggled",
        "clear" => "Cleared",
        _ => "Updated",
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use cmdr_fs::entry::TagRef;

    fn tag(name: &str, color: u8) -> TagRef {
        TagRef {
            name: name.to_string(),
            color,
        }
    }

    #[test]
    fn color_index_maps_the_seven_names() {
        assert_eq!(color_index("red"), Some(6));
        assert_eq!(color_index("Orange"), Some(7));
        assert_eq!(color_index("gray"), Some(1));
        assert_eq!(color_index("grey"), Some(1)); // alias
        assert_eq!(color_index("mauve"), None);
    }

    #[test]
    fn set_keeps_colorless_tags_and_adds_canonical_color() {
        // A custom colorless "Important" tag survives; blue gets its canonical tag.
        let current = vec![tag("Important", 0)];
        let desired = desired_tags_for_set(&current, &[4]);
        assert_eq!(desired, vec![tag("Important", 0), tag("Blue", 4)]);
    }

    #[test]
    fn set_preserves_a_custom_named_tag_of_a_requested_color() {
        // A custom red-colored tag is kept (name preserved) rather than replaced
        // by the canonical "Red".
        let current = vec![tag("Urgent", 6)];
        let desired = desired_tags_for_set(&current, &[6]);
        assert_eq!(desired, vec![tag("Urgent", 6)]);
    }

    #[test]
    fn set_drops_colored_tags_not_requested() {
        let current = vec![tag("Red", 6), tag("Blue", 4)];
        let desired = desired_tags_for_set(&current, &[4]);
        assert_eq!(desired, vec![tag("Blue", 4)]);
    }

    /// #541: `set` merges with the file's existing tags, so one it can't decode is
    /// skipped and named in the result, its bytes untouched.
    #[test]
    fn set_and_toggle_skip_a_file_with_undecodable_tags() {
        use crate::file_system::tags::TAGS_XATTR;
        let dir = crate::test_support::TestDir::new(&format!("mcp_tags_skip_{}", std::process::id()));
        let bad = dir.join("bad.txt");
        let good = dir.join("good.txt");
        std::fs::write(&bad, b"x").unwrap();
        std::fs::write(&good, b"x").unwrap();
        xattr::set(&bad, TAGS_XATTR, b"garbage").unwrap();
        let paths = vec![bad.to_string_lossy().into_owned(), good.to_string_lossy().into_owned()];

        for action in ["set", "toggle"] {
            let outcome = apply_tag_action(action, &paths, &[6, 4]).unwrap();
            assert_eq!(
                xattr::get(&bad, TAGS_XATTR).unwrap().as_deref(),
                Some(&b"garbage"[..]),
                "{action} must leave the undecodable value alone"
            );
            assert_eq!(outcome.skipped.len(), 1, "one skip reported by {action}");
            assert_eq!(outcome.skipped[0].path, paths[0]);
            assert_eq!(outcome.updates.len(), 1, "{action} still writes the readable file");
            let note = skipped_note(&outcome.skipped);
            assert!(note.contains(&paths[0]), "the result names the skipped file: {note}");
        }
        assert_eq!(skipped_note(&[]), "");
    }

    #[test]
    fn set_dedupes_repeated_requested_colors() {
        let desired = desired_tags_for_set(&[], &[6, 6]);
        assert_eq!(desired, vec![tag("Red", 6)]);
    }
}
