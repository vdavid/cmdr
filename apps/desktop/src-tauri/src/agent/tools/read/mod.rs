//! The Ask Cmdr agent's concrete read-only tool handlers, one file per family:
//!
//! - [`state`]: `app_state` — the live pane + volume snapshot.
//! - [`listing`]: `list_dir` — drive-index listing, sortable by size (the disk-usage question).
//! - [`importance`]: `important_folders` + `folder_importance` — the offline importance signal.
//! - [`volumes`]: `list_volumes` — every volume with freshness + connectivity.
//! - [`inspect`]: `inspect_file` — one file's metadata, sniffed format, and a bounded content window.
//!
//! The `operations_list` / `operations_get` family reuses the ai-client executors
//! unchanged (shared registry entries), so it has no file here.
//!
//! Each handler REUSES a shipped, deterministic core (the indexing queries, the
//! importance read API, `snapshot_volumes`) rather than re-deriving listing or
//! scoring logic, and voices its coverage caveats in the typed result (spec §2.4:
//! honesty is load-bearing). See `../DETAILS.md` for the catalog and the
//! reuse/honesty rules.

pub mod importance;
pub mod inspect;
pub mod listing;
pub mod pane_listing;
pub mod state;
pub mod volumes;

/// Expand a leading `~` to `$HOME` (agents routinely send `~/Documents`). Only the
/// tilde prefix — no `~user` form. Mirrors the importance resource's handling so an
/// agent's paths resolve the same way across tools.
pub(crate) fn expand_tilde(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if path == "~" {
        return home;
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => path.to_string(),
    }
}

/// `serde(skip_serializing_if)` predicate for a `bool` field that should only
/// serialize when `true` (keeps the model-facing JSON terse). The stdlib `Not::not`
/// takes `bool` by value, not `&bool`, so serde needs this shim.
pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

/// `serde(skip_serializing_if)` predicate for a count that carries no information
/// when it's zero (an `offset` nobody paged past).
pub(crate) fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Join a child name onto a parent directory path, avoiding a doubled slash at the
/// filesystem root. The parent is already tilde-expanded and index-normalized.
pub(crate) fn join_child_path(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_child_path_never_doubles_the_root_slash() {
        assert_eq!(join_child_path("/", "Users"), "/Users");
        assert_eq!(join_child_path("/Users/x", "Documents"), "/Users/x/Documents");
    }
}
