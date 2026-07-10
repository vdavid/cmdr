//! The `tag` tool: set macOS Finder color tags on files.
//!
//! Thin adapter over `file_system::tags` (`toggle_color` / `set_tags`) — the same
//! primitives the context-menu tag toggle uses. It resolves target paths off the
//! pane state (names / selection / cursor, via `resolve_pane_target_paths`), then
//! patches the cached listing so the panes re-render the colored dots (reusing
//! `apply_tags_to_listing`, the enrich path). Gate `Always`: silent metadata
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

    let updates = apply_tag_action(action, &paths, &colors)?;
    refresh_listing_tags(&state, updates);

    // Flush the pane's MCP state so the `[tags:…]` marker shows in cmdr://state
    // promptly instead of ~2 s later (the debounced sync). `refresh_listing_tags`
    // patched the backend `LISTING_CACHE` synchronously, and the FE's
    // `syncPaneStateToMcp` re-reads that cache via `getFileAt` — so the forced push
    // carries the new tags without racing the 50 ms-coalesced `directory-diff`.
    // Best-effort: the tags already landed on disk and in the cache, so a flush
    // timeout just delays the marker to the next push rather than failing the tool.
    let _ = super::flush_pane_state(app, &pane).await;

    Ok(json!(format!(
        "OK: {} tags on {} in the {pane} pane.",
        action_verb(action),
        crate::pluralize::pluralize(paths.len() as u64, "item")
    )))
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
/// caller can patch the listing cache. `set` preserves colorless (custom-named)
/// tags and any existing colored tag whose color is requested; `toggle` uses
/// Finder's all-have→remove rule per color; `clear` removes every tag.
#[cfg(target_os = "macos")]
fn apply_tag_action(
    action: &str,
    paths: &[String],
    colors: &[u8],
) -> Result<Vec<(String, Vec<crate::file_system::listing::metadata::TagRef>)>, ToolError> {
    use crate::file_system::tags::{read_tags, set_tags, toggle_color};
    use std::path::Path;

    match action {
        "clear" => {
            let mut updates = Vec::with_capacity(paths.len());
            for p in paths {
                set_tags(Path::new(p), &[])
                    .map_err(|e| ToolError::internal(format!("Couldn't clear tags on {p}: {e}")))?;
                updates.push((p.clone(), Vec::new()));
            }
            Ok(updates)
        }
        "toggle" => {
            // Toggle each requested color across all paths. Each `toggle_color`
            // reads current on-disk state, so the last color's result reflects
            // every prior toggle — it's the refresh snapshot. `colors` is
            // validated non-empty for toggle, so `last` is always assigned.
            let mut last = Vec::new();
            for &color in colors {
                last =
                    toggle_color(paths, color).map_err(|e| ToolError::internal(format!("Couldn't toggle tag: {e}")))?;
            }
            Ok(last)
        }
        "set" => {
            let mut updates = Vec::with_capacity(paths.len());
            for p in paths {
                let current = read_tags(Path::new(p));
                let desired = desired_tags_for_set(&current, colors);
                // Skip files already in the target state so we don't churn mtimes.
                if desired != current {
                    set_tags(Path::new(p), &desired)
                        .map_err(|e| ToolError::internal(format!("Couldn't set tags on {p}: {e}")))?;
                }
                updates.push((p.clone(), desired));
            }
            Ok(updates)
        }
        _ => Err(ToolError::internal("unreachable: action validated by caller")),
    }
}

/// The desired tag set for `set`: keep every colorless (custom-named) tag, keep an
/// existing colored tag whose color is requested (preserving a custom name), drop
/// colored tags whose color isn't requested, and add the canonical system tag for
/// a requested color the file lacks. Pure, so it's unit-testable.
#[cfg(target_os = "macos")]
fn desired_tags_for_set(
    current: &[crate::file_system::listing::metadata::TagRef],
    colors: &[u8],
) -> Vec<crate::file_system::listing::metadata::TagRef> {
    use crate::file_system::listing::metadata::TagRef;
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
/// re-render immediately, reusing the same `apply_tags_to_listing` path the
/// context-menu toggle and `enrich_tags` use. No matching listing (a rare
/// path/volume mismatch) simply skips the in-place refresh — the tags still
/// landed on disk and the dots update on the next visible-range enrich.
#[cfg(target_os = "macos")]
fn refresh_listing_tags(
    state: &crate::mcp::pane_state::PaneState,
    updates: Vec<(String, Vec<crate::file_system::listing::metadata::TagRef>)>,
) {
    if updates.is_empty() {
        return;
    }
    let volume_id = state.volume_id.as_deref().unwrap_or("root");
    let listing = crate::file_system::listing::caching::snapshot_listings()
        .into_iter()
        .find(|l| l.volume_id == volume_id && l.path.to_string_lossy() == state.path);
    if let Some(listing) = listing {
        crate::file_system::listing::caching::apply_tags_to_listing(&listing.listing_id, updates);
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
    use crate::file_system::listing::metadata::TagRef;

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

    #[test]
    fn set_dedupes_repeated_requested_colors() {
        let desired = desired_tags_for_set(&[], &[6, 6]);
        assert_eq!(desired, vec![tag("Red", 6)]);
    }
}
