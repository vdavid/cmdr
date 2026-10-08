//! The `cmdr://state` pane blocks: tabs, the summary fields, the cursor, and the compact file lines, plus the
//! pane state used by ordinary MCP resources.

use super::StateOptions;
use crate::mcp::pane_state::{PaneFileEntry, PaneListing, PaneState, TabInfo};
use crate::search::format_size;

/// The `listing:` value for a pane whose listing isn't settled, or `None` for one that is.
pub(crate) fn listing_marker(listing: PaneListing) -> Option<&'static str> {
    match listing {
        PaneListing::Settled => None,
        PaneListing::Loading => Some("loading"),
        PaneListing::Stalled => Some("stalled (the volume isn't answering; retrying in the background)"),
        PaneListing::Error => Some("error (see recentErrors)"),
    }
}

/// Format a file entry in compact format.
/// Format: `i:INDEX TYPE NAME [SIZE] [DATES] [MARKERS]`
pub(crate) fn format_file_compact(
    file: &PaneFileEntry,
    index: usize,
    is_cursor: bool,
    is_selected: bool,
    include_details: bool,
) -> String {
    let file_type = if file.is_directory {
        "d"
    } else if file.path.contains(" -> ") {
        "l" // symlink indicated by arrow in path
    } else {
        "f"
    };

    let mut parts = vec![format!("i:{} {} {}", index, file_type, file.name)];

    if include_details {
        if let Some(size) = file.size {
            parts.push(format_size(size));
        } else if let Some(recursive_size) = recursive_size_text(file) {
            parts.push(recursive_size);
            if let Some(marker) = on_disk_marker(file.recursive_size, file.recursive_physical_size) {
                parts.push(marker);
            }
        }
        if let Some(ref modified) = file.modified {
            parts.push(modified.clone());
        }
    }

    if is_cursor {
        parts.push("[cur]".to_string());
    }
    if is_selected {
        parts.push("[sel]".to_string());
    }
    // The recursive size is still moving (a walk is on, above, or below this dir,
    // or its own index writes are draining). Mirrors the per-row "size updating"
    // hourglass. "Unsettled" rather than "pending": "pending" reads as a small
    // refinement queued up, and this number can be off by orders of magnitude in
    // either direction until the walk that's rewriting it finishes.
    if file.recursive_size_updating == Some(true) {
        parts.push("[size-unsettled]".to_string());
    }
    // Exact, but computed at an older volume epoch. A status like the hourglass
    // above, so it shows with or without details.
    if file.recursive_size_stale == Some(true) {
        parts.push("[size-stale]".to_string());
    }
    if let Some(marker) = tags_marker(&file.tags) {
        parts.push(marker);
    }

    parts.join(" ")
}

/// Prefix for a SETTLED directory total the indexer hasn't finished covering.
/// Same glyph as the UI's `LOWER_BOUND_GLYPH` (`full-list-utils.ts`).
const LOWER_BOUND_GLYPH: &str = "≥";

/// Prefix for a total that's still moving: approximate, direction unknown.
const IN_FLUX_GLYPH: &str = "~";

/// The size cell for a directory's recursive total, or `None` when there's
/// nothing honest to print.
///
/// Mirrors the UI's `getDirSizeDisplayState`: an incomplete subtree is a lower
/// bound and says so, and an incomplete subtree with no bytes known yet prints
/// nothing at all (the UI's `<dir>` placeholder) because `≥0 B` reads as a
/// measurement. An absent `recursive_size_complete` means exact, which covers
/// fixtures and volumes with no index.
///
/// **Motion outranks coverage.** `≥` claims a floor, which only holds once the
/// number has stopped: it's derived from unscanned subtrees alone and can't
/// express the opposite error, an index entry for a subtree that's already gone,
/// where the truth is far LOWER. A walk is busy correcting exactly that, so an
/// in-flux total wears `~` instead. (The UI resolves the same collision by
/// dropping its `≥` and leaving the hourglass to speak; a third glyph would
/// compete with the hourglass in a dense column, which is why the two surfaces
/// share the rule but not the symbol.)
///
/// Why this matters more here than on screen: a person sees a folder mid-scan
/// and waits, while an agent reads the number and acts on it.
pub(crate) fn recursive_size_text(file: &PaneFileEntry) -> Option<String> {
    let size = file.recursive_size?;
    let complete = file.recursive_size_complete.unwrap_or(true);
    if !complete && size == 0 {
        return None;
    }
    let glyph = match (file.recursive_size_updating == Some(true), complete) {
        (true, _) => IN_FLUX_GLYPH,
        (false, false) => LOWER_BOUND_GLYPH,
        (false, true) => "",
    };
    Some(format!("{glyph}{}", format_size(size)))
}

/// `(1 GB on disk)` when the allocated-blocks total diverges from the logical
/// total enough to change a decision (compression, sparse files, cloud
/// placeholders), else `None` so the common case costs nothing.
///
/// Thresholds mirror the UI's `hasSizeMismatch` (`full-list-utils.ts`): BOTH a
/// ≥50% relative gap AND a ≥200 MB absolute one. Keeping the two surfaces on
/// one rule is what stops `cmdr://state` and the file list from disagreeing
/// about which folders are worth a second look.
pub(crate) fn on_disk_marker(logical: Option<u64>, physical: Option<u64>) -> Option<String> {
    let (logical, physical) = (logical?, physical?);
    // A zero on either side is "unknown", not "empty": cloud placeholders read
    // physical 0 with a real logical size, and both are noise here.
    if logical == 0 || physical == 0 {
        return None;
    }
    let diff = logical.abs_diff(physical);
    let smaller = logical.min(physical);
    // `diff * 2 >= smaller` rather than `diff >= smaller / 2`: integer division
    // would round the threshold down on odd byte counts.
    (diff.saturating_mul(2) >= smaller && diff >= 200_000_000).then(|| format!("({} on disk)", format_size(physical)))
}

/// The `[tags:...]` marker for a file's Finder tags, or `None` when it has none
/// (zero cost in the common case). Colored tags render as their color name (the
/// dot the UI shows); a colorless custom tag renders as its own name. Pure, so
/// it's unit-testable.
pub(crate) fn tags_marker(tags: &[crate::file_system::listing::metadata::TagRef]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let labels: Vec<String> = tags
        .iter()
        .map(|t| match tag_color_name(t.color) {
            Some(color) => color.to_string(),
            None => t.name.clone(),
        })
        .collect();
    Some(format!("[tags:{}]", labels.join(",")))
}

/// The lowercase color name for a Finder color index (1..=7), or `None` for the
/// colorless index 0.
fn tag_color_name(color: u8) -> Option<&'static str> {
    Some(match color {
        1 => "gray",
        2 => "green",
        3 => "purple",
        4 => "blue",
        5 => "yellow",
        6 => "red",
        7 => "orange",
        _ => return None,
    })
}

/// Build YAML for a single pane.
///
/// When `opts.compact` is true, omits the `files:` list (the largest source of
/// YAML volume in the default state read) while keeping every summary field.
/// The per-pane `cursor`, `totalFiles`, and `loadedRange` still show, so
/// callers can still tell where the cursor is without paying for 100 file
/// lines.
pub(crate) fn build_pane_yaml_with_options(state: &PaneState, indent: &str, opts: &StateOptions) -> String {
    let compact = opts.compact;
    let mut lines = Vec::new();

    // Tabs (first, gives context for which tab is active before showing its content)
    if !state.tabs.is_empty() {
        lines.push(format!("{}tabs:", indent));
        for (idx, tab) in state.tabs.iter().enumerate() {
            let formatted = format_tab_compact(tab, idx);
            lines.push(format!("{}  - {}", indent, formatted));
        }
    }

    // Volume and path
    lines.push(format!(
        "{}volume: {}",
        indent,
        state.volume_name.as_deref().unwrap_or("unknown")
    ));
    if let Some(ref vid) = state.volume_id {
        lines.push(format!("{}volumeId: {}", indent, vid));
    }
    lines.push(format!("{}path: {}", indent, state.path));
    lines.push(format!("{}view: {}", indent, state.view_mode));
    // The pane always pushes both halves explicitly (`pane-mcp-sync.svelte.ts`),
    // INCLUDING the `relevance:desc` a search-results pane in the engine's ranked
    // order reports. These fallbacks therefore describe only a `PaneState` no pane
    // ever pushed (a default-constructed one at startup); ❌ don't lean on them to
    // stand in for a real pane's order, which is how a ranked result set used to
    // come out claiming `name:asc`.
    lines.push(format!(
        "{}sort: \"{}:{}\"",
        indent,
        if state.sort_field.is_empty() {
            "name"
        } else {
            &state.sort_field
        },
        if state.sort_order.is_empty() {
            "asc"
        } else {
            &state.sort_order
        }
    ));
    lines.push(format!("{}totalFiles: {}", indent, state.total_files));
    // Only when it isn't settled, so the common case stays clean: this is what tells
    // an empty folder (`totalFiles` counting none) from one that's still coming.
    if let Some(listing) = listing_marker(state.listing) {
        lines.push(format!("{}listing: {}", indent, listing));
    }
    lines.push(format!(
        "{}loadedRange: [{}, {}]",
        indent, state.loaded_start, state.loaded_end
    ));

    // Cursor info. `cursor_index` is global while `files` holds only the loaded
    // window, so the detail lookup is window-relative; a cursor outside the
    // window shows no details rather than a wrong file's.
    lines.push(format!("{}cursor:", indent));
    lines.push(format!("{}  index: {}", indent, state.cursor_index));
    let cursor_window_index = state.cursor_index.checked_sub(state.loaded_start);
    if state.view_mode == "brief"
        && let Some(cursor_file) = cursor_window_index.and_then(|i| state.files.get(i))
    {
        lines.push(format!("{}  name: {}", indent, cursor_file.name));
        if let Some(size) = cursor_file.size {
            lines.push(format!("{}  size: {}", indent, format_size(size)));
        } else if let Some(recursive_size) = cursor_file.recursive_size {
            lines.push(format!("{}  size: {}", indent, format_size(recursive_size)));
        }
        if let Some(ref modified) = cursor_file.modified {
            lines.push(format!("{}  modified: {}", indent, modified));
        }
    }

    // Selected count
    lines.push(format!("{}selected: {}", indent, state.selected_indices.len()));

    // Quick filter: only while one narrows the pane, which makes every row below
    // a FILTERED row.
    if let Some(ref pattern) = state.quick_filter {
        lines.push(format!("{}quickFilter: {:?}", indent, pattern));
    }

    // Type-to-jump state: only emitted while a buffer or visible indicator
    // exists, so the YAML stays clean during the common case.
    if let Some(ref ttj) = state.type_to_jump {
        lines.push(format!("{}typeToJump:", indent));
        lines.push(format!("{}  buffer: {:?}", indent, ttj.buffer));
        lines.push(format!("{}  indicatorVisible: {}", indent, ttj.indicator_visible));
        lines.push(format!("{}  indicatorStale: {}", indent, ttj.indicator_stale));
        if let Some(ref name) = ttj.last_matched_name {
            lines.push(format!("{}  lastMatchedName: {}", indent, name));
        }
    }

    // A mount the pane couldn't open. Emitted right after the summary fields so
    // it can't be mistaken for a note about the listing below it: while this is
    // set, `path` and `files` describe the share list the error pane replaced.
    if let Some(ref err) = state.mount_error {
        lines.push(format!("{}mountError:", indent));
        lines.push(format!("{}  share: {:?}", indent, err.share));
        lines.push(format!("{}  reason: {}", indent, err.reason));
        lines.push(format!("{}  message: {:?}", indent, err.message));
    }

    // Files list
    if compact {
        // `compact` callers care about path / volumeId / cursor / totalFiles, not
        // the 100-entry virtual-scroll window. Emit a single placeholder so the
        // YAML still shows the section exists.
        lines.push(format!("{}files: <omitted: compact=true>", indent));
    } else {
        lines.push(format!("{}files:", indent));

        let is_full_mode = state.view_mode == "full";
        let selected_set: std::collections::HashSet<usize> = state.selected_indices.iter().copied().collect();

        for (idx, file) in state.files.iter().enumerate() {
            // Convert local index to global index based on loaded_start
            let global_idx = state.loaded_start + idx;
            let is_cursor = global_idx == state.cursor_index;
            let is_selected = selected_set.contains(&global_idx);
            // In full mode, include details for all files. In brief mode, only for cursor.
            let include_details = is_full_mode || is_cursor;
            let formatted = format_file_compact(file, global_idx, is_cursor, is_selected, include_details);
            lines.push(format!("{}  - \"{}\"", indent, formatted));
        }
    }

    lines.join("\n")
}

/// Format a tab entry in compact format.
/// Format: `i:INDEX id:TAB_ID [active] [pinned] FolderName (/full/path)`
pub(crate) fn format_tab_compact(tab: &TabInfo, index: usize) -> String {
    let folder_name = tab.path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(&tab.path);

    let mut markers = Vec::new();
    if tab.active {
        markers.push("[active]");
    }
    if tab.pinned {
        markers.push("[pinned]");
    }

    if markers.is_empty() {
        format!("i:{} id:{} {} ({})", index, tab.id, folder_name, tab.path)
    } else {
        format!(
            "i:{} id:{} {} {} ({})",
            index,
            tab.id,
            markers.join(" "),
            folder_name,
            tab.path
        )
    }
}
