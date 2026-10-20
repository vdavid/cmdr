//! The `operations:` section of `cmdr://state`: every queued, running, or paused
//! write operation (copy / move / delete / trash / compress / archive edit) with
//! live progress, speed, and ETA where it's running, plus any failure the user
//! hasn't dismissed.
//!
//! This is the discovery surface for the `queue` tool — an agent reads it to find
//! operation ids and their lifecycle status before pausing / resuming / cancelling.
//!
//! **Two-source join.** Membership + lifecycle status (`running` / `paused` /
//! `queued` / `failed`) come from the operation manager's registry
//! (`list_operations`, whose `OperationSnapshot` carries no progress by design),
//! while progress / speed / ETA come from the separate write-operations status
//! cache (`get_operation_status`, the same single-source the progress dialog
//! feeds from). Joined by operation id; a queued op simply has no progress
//! fields, and so does a retained `failed` row (its op is over). Completed and
//! cancelled ops are gone from both sources — see `terminal_ops` for the ring
//! that keeps their outcome for `await operation_complete`.
//!
//! Speed and ETA are whole-run averages (`bytes_done / elapsed`), not the
//! progress dialog's EWMA — good enough for an agent's "is it moving, roughly
//! when is it done" questions.
//!
//! **A read alone answers "why isn't this moving?".** `status` separates queued
//! / running / paused / failed, `waitingOn` separates the ways a RUNNING row can
//! be stopped (destination, source, an unanswered clash, or the wedge shape that
//! explains itself with nothing), and `pendingConflict` carries the `conflictId`
//! an answer needs. `pendingSourcesGone` and `pendingLockedSources` are the other
//! questions a running row can be parked on: a queued operation whose sources left
//! while it waited, and a move asking before any byte moves whether to unlock the
//! user's Locked items. No `write-progress` subscription required: the wait
//! rides `OperationStatus` and is classified at read time.
//!
//! **Where the work is.** A transfer's row also names the folders it's in and
//! the files in flight (`activeFolders:`), read off the live transfer tree's
//! follow-the-work window, the third source of the join.

use crate::pluralize::pluralize;
use crate::search::query::format_size;
use cmdr_operation_log::types::OperationId;
use cmdr_operations::{
    LifecycleStatus, OperationSnapshot, OperationStatus, TransferActivity, TransferTreeRow, TransferTreeView,
    TransferTreeViewRequest, TreeRowKind, TreeRowState, WriteConflictEvent, WriteLockedSourcesEvent,
    WriteSourcesGoneEvent,
};

/// One row of the `operations:` section: the registry's membership / status /
/// summary joined with the status cache's live progress (absent for queued ops)
/// and the clash it's parked on (absent unless it's asking).
pub(crate) struct OperationRow {
    pub snapshot: OperationSnapshot,
    pub progress: Option<OperationStatus>,
    pub pending_conflict: Option<WriteConflictEvent>,
    /// The sources-gone question a queued operation is parked on, if any.
    pub pending_sources_gone: Option<WriteSourcesGoneEvent>,
    /// The Locked-items question a move is parked on, if any.
    pub pending_locked_sources: Option<WriteLockedSourcesEvent>,
    /// The live transfer tree, following the work: absent for an operation
    /// that builds none or isn't underway.
    pub tree: Option<TransferTreeView>,
}

/// Snapshot every registered operation (queued / running / paused), joining the
/// registry membership with the live progress cache by id, plus any Stop-mode
/// clash the operation is parked on.
pub(crate) fn snapshot_operations() -> Vec<OperationRow> {
    // Before `install` nothing is registered.
    let Ok(engine) = crate::operations_host::operations() else {
        return Vec::new();
    };
    engine
        .list_operations()
        .into_iter()
        .map(|snapshot| {
            let operation_id = OperationId::from(snapshot.operation_id.as_str());
            let progress = engine.get_operation_status(&operation_id);
            let pending_conflict = engine.pending_write_conflict(&operation_id);
            let pending_sources_gone = engine.pending_sources_gone(&operation_id);
            let pending_locked_sources = engine.pending_locked_sources(&operation_id);
            // Only while it runs or is paused: a queued or failed op has no tree to
            // show, and asking would take a lock for nothing.
            let tree = progress
                .is_some()
                .then(|| {
                    engine
                        .transfer_tree_view(&operation_id, &TransferTreeViewRequest::following_the_work())
                        .ok()
                })
                .flatten();
            OperationRow {
                snapshot,
                progress,
                pending_conflict,
                pending_sources_gone,
                pending_locked_sources,
                tree,
            }
        })
        .collect()
}

/// The lower-case lifecycle token. `queued` / `running` / `paused` come from a
/// live record; `failed` comes from a retained failure the user hasn't dismissed
/// yet (`crates/cmdr-operations/DETAILS.md` § "Retained failures").
fn status_token(status: LifecycleStatus) -> &'static str {
    match status {
        LifecycleStatus::Queued => "queued",
        LifecycleStatus::Running => "running",
        LifecycleStatus::Paused => "paused",
        LifecycleStatus::Failed => "failed",
        // Unreachable: a completed or cancelled op leaves the registry and is
        // retained nowhere. Rendered honestly rather than papered over if one
        // ever slips through.
        LifecycleStatus::Done => "done",
        LifecycleStatus::Cancelled => "cancelled",
    }
}

/// Pure YAML builder for the `operations:` section. `now_ms` is injected so tests
/// don't depend on the wall clock.
pub(crate) fn build_operations_yaml(rows: &[OperationRow], now_ms: u64) -> String {
    if rows.is_empty() {
        return "operations: []\n".to_string();
    }
    let mut yaml = String::from("operations:\n");
    for row in rows {
        let kind = enum_str(serde_json::to_value(row.snapshot.operation_type).ok());
        let status = status_token(row.snapshot.status);
        yaml.push_str(&format!(
            "  - operationId: {}\n    type: {kind}\n    status: {status}\n",
            row.snapshot.operation_id
        ));
        // Source / destination summaries help the agent tell operations apart.
        // Verbatim: they're the names the queue window shows the user, and an
        // agent must be able to name what's being copied when the user asks
        // (`resources/DETAILS.md` § "What renders verbatim").
        if let Some(source) = &row.snapshot.source {
            yaml.push_str(&format!("    source: {source:?}\n"));
        }
        if let Some(destination) = &row.snapshot.destination {
            yaml.push_str(&format!("    destination: {destination:?}\n"));
        }
        // Why a retained `failed` row stopped. The typed variant's own name, the
        // same one the frontend renders its wording from — ❌ never a sentence.
        // Without it an agent reads `status: failed` and has to guess between a
        // vanished source, a full disk, and a permission refusal, which decide
        // completely different next moves.
        if let Some(error) = &row.snapshot.error {
            yaml.push_str(&format!("    error: {}\n", error_line(error)));
        }
        // Live progress: present for running / paused, absent for queued (the
        // status cache has no entry until the op is admitted and starts work).
        if let Some(op) = &row.progress {
            yaml.push_str(&format!("    progress: {}\n", progress_line(op)));
            if let Some(file) = &op.current_file {
                // Verbatim, like the summaries above: "which file is it on?" is
                // a question the user asks through the agent.
                yaml.push_str(&format!("    currentFile: {file:?}\n"));
            }
            let elapsed_s = now_ms.saturating_sub(op.started_at) / 1000;
            if elapsed_s > 0 && op.bytes_done > 0 {
                let rate = op.bytes_done / elapsed_s;
                yaml.push_str(&format!("    speed: {}/s\n", format_size(rate)));
                if op.bytes_total > op.bytes_done && rate > 0 {
                    let eta_s = (op.bytes_total - op.bytes_done) / rate;
                    yaml.push_str(&format!("    etaSeconds: {eta_s}\n"));
                }
            }
            yaml.push_str(&format!("    elapsedSeconds: {elapsed_s}\n"));
            yaml.push_str(&activity_lines(op.activity.as_ref()));
        }
        // The clash the operation is parked on. `waitingOn: conflict` above says
        // there IS a question; this says which one, and carries the id an answer
        // has to name.
        if let Some(conflict) = &row.pending_conflict {
            yaml.push_str(&pending_conflict_yaml(conflict));
        }
        if let Some(tree) = &row.tree {
            yaml.push_str(&active_folders_yaml(tree));
        }
        if let Some(question) = &row.pending_sources_gone {
            yaml.push_str(&pending_sources_gone_yaml(question));
        }
        if let Some(question) = &row.pending_locked_sources {
            yaml.push_str(&pending_locked_sources_yaml(question));
        }
    }
    yaml
}

/// The `waitingOn:` line and its two supporting numbers: the answer to "this row
/// says `running` and nothing is moving — why?".
///
/// `status: running` alone can't separate a slow copy, a wedged mount, a
/// transfer parked on a conflict prompt, and an operation queued behind a lane;
/// they all poll identically. `waitingOn` splits the first three (`queued` is
/// already the row's `status`), and `unknown` keeps the fourth shape — nothing
/// moving, nothing explaining it — from hiding among the waits that DO have a
/// reason.
///
/// Absent when the backend can't classify itself (a local copy keeps no
/// in-flight table). ❌ Never rendered as a stand-in `moving`: an agent acts on
/// this.
fn activity_lines(activity: Option<&TransferActivity>) -> String {
    let Some(activity) = activity else {
        return String::new();
    };
    let mut yaml = format!(
        "    waitingOn: {}\n",
        enum_str(serde_json::to_value(activity.waiting_on).ok())
    );
    // Both only when they carry a finding. A zero stillness is what a moving
    // transfer always reports, and a zero in-flight count is what an operation
    // with no table reports — rendering either would be noise an agent reads as
    // a measurement.
    if activity.still_for_seconds > 0 {
        yaml.push_str(&format!("    stillForSeconds: {}\n", activity.still_for_seconds));
    }
    if activity.in_flight > 0 {
        yaml.push_str(&format!("    inFlight: {}\n", activity.in_flight));
    }
    // Same rule: only when true / present. Together they separate "the source
    // hasn't sent the first byte yet" and "a response is still arriving" from a
    // wedge, which `waitingOn: source` alone can't.
    if activity.opening_source {
        yaml.push_str("    openingSource: true\n");
    }
    if let Some(rate) = activity.source_inbound_bytes_per_second {
        yaml.push_str(&format!("    sourceInboundBytesPerSecond: {rate}\n"));
    }
    // Which volume `waitingOn: reconnecting` is waiting for. The name the user
    // gave it (a NAS, a server), the same one the volume list shows, so it's
    // quoted rather than redacted like a path.
    if let Some(volume) = &activity.reconnecting_volume {
        yaml.push_str(&format!("    reconnectingVolume: {volume:?}\n"));
    }
    yaml
}

/// The `pendingConflict:` block: what is being asked, and the id an answer has
/// to name. The `conflictId` is the load-bearing part — `resolve_conflict`
/// requires it, so an answer can't land on a clash the operation has since
/// moved past.
fn pending_conflict_yaml(conflict: &WriteConflictEvent) -> String {
    let mut yaml = String::from("    pendingConflict:\n");
    yaml.push_str(&format!("      conflictId: {}\n", conflict.conflict_id.0));
    // Verbatim, like the pane's own `path:` and `files:` in this same resource
    // (`resources/mod.rs`) and the row's summaries above. `redact_line` turns a
    // path into `/tmp/<dir>/<file>.txt`, and a clash an agent can't name is a clash it can't answer: it would be
    // choosing between skip and overwrite for a file it can't see, over a
    // decision that destroys data. Nothing is protected by hiding it either —
    // the same read already lists that directory by name.
    yaml.push_str(&format!("      source: {:?}\n", conflict.source_path));
    yaml.push_str(&format!("      destination: {:?}\n", conflict.destination_path));
    yaml.push_str(&format!("      sourceSize: {}\n", optional_size(conflict.source_size)));
    yaml.push_str(&format!(
        "      destinationSize: {}\n",
        optional_size(conflict.destination_size)
    ));
    yaml.push_str(&format!(
        "      destinationIsNewer: {}\n",
        conflict.destination_is_newer
    ));
    // The two paths can print identically: the destination holds the name in
    // another Unicode spelling, and an overwrite replaces THAT entry.
    yaml.push_str(&format!(
        "      destinationIsLookAlike: {}\n",
        conflict.destination_is_look_alike
    ));
    yaml.push_str(
        "      answerWith: resolve_conflict (operationId + conflictId + resolution skip|overwrite|rename|overwrite_smaller|overwrite_older, optional applyToAll)\n",
    );
    yaml
}

/// The `pendingSourcesGone:` block: a queued operation found some of its sources
/// gone when its turn came and waits for someone to say whether to carry on with
/// the rest. The paths render verbatim for the same reason `pendingConflict`'s
/// do: the decision is about THOSE files.
fn pending_sources_gone_yaml(question: &WriteSourcesGoneEvent) -> String {
    let mut yaml = String::from("    pendingSourcesGone:\n");
    yaml.push_str(&format!("      goneCount: {}\n", question.gone_count));
    yaml.push_str(&format!("      totalCount: {}\n", question.total_count));
    yaml.push_str("      gone:\n");
    for path in &question.gone_sample {
        yaml.push_str(&format!("        - {path:?}\n"));
    }
    yaml.push_str("      answerWith: queue (action continue_without_gone to carry on with the rest, or cancel)\n");
    yaml
}

/// The `pendingLockedSources:` block: a move found the user's Locked items among
/// what it moves and waits, before any byte moves, for someone to say whether to
/// unlock them for it. Paths verbatim, like `pendingSourcesGone`.
fn pending_locked_sources_yaml(question: &WriteLockedSourcesEvent) -> String {
    let mut yaml = String::from("    pendingLockedSources:\n");
    yaml.push_str(&format!("      lockedCount: {}\n", question.locked_count));
    yaml.push_str("      locked:\n");
    for path in &question.locked_sample {
        yaml.push_str(&format!("        - {path:?}\n"));
    }
    yaml.push_str(
        "      answerWith: queue (action move_locked to unlock them for the move, skip_locked to leave them, or cancel)\n",
    );
    yaml
}

/// A size the backend couldn't determine reads as `unknown`, ❌ never as a `0`
/// an agent would compare against.
fn optional_size(size: Option<u64>) -> String {
    size.map_or_else(|| "unknown".to_string(), format_size)
}

/// One human line: bytes and file counters, with a percent when totals are known.
/// Totals are 0 while scanning, so each part only renders once it's meaningful.
fn progress_line(op: &OperationStatus) -> String {
    counts_line(
        op.bytes_done,
        op.bytes_total,
        op.files_done as u64,
        op.files_total as u64,
    )
    .unwrap_or_else(|| "scanning".to_string())
}

/// The bytes and files parts of a progress line, `None` when neither is known
/// yet. Shared by the operation's line and each active folder's, so the two
/// read alike.
fn counts_line(bytes_done: u64, bytes_total: u64, files_done: u64, files_total: u64) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if bytes_total > 0 {
        let percent = ((bytes_done as f64 / bytes_total as f64) * 100.0).min(100.0) as u8;
        parts.push(format!(
            "{} / {} ({percent}%)",
            format_size(bytes_done),
            format_size(bytes_total)
        ));
    } else if bytes_done > 0 {
        parts.push(format!("{} so far", format_size(bytes_done)));
    }
    if files_total > 0 {
        parts.push(format!("{files_done}/{}", pluralize(files_total, "file")));
    } else if files_done > 0 {
        parts.push(format!("{} so far", pluralize(files_done, "file")));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// The most in-flight files one row lists. A parallel SMB copy can have dozens
/// moving at once, and `cmdr://state` is read constantly; the rest are counted.
const MAX_ACTIVE_FILES: usize = 8;

/// The `activeFolders:` block: the folders the work is in, each with its own
/// progress and its files in flight, so an agent can say "it's in
/// `Photos 2024/Prague`, 47 of 60 files done".
///
/// Read off the transfer tree's follow-the-work window (the same
/// `get_transfer_tree_view` answer the progress dialog renders): the window
/// starts at the topmost active row, and its `ancestors` name the folders above
/// that row, so walking the rows with a by-depth stack gives every active file
/// its folder path. Names render verbatim, like `pendingConflict:`'s paths: a
/// redacted `<dir>/<file>.jpg` answers nothing, and they're the relative names
/// of what the user selected, which `panes:` already lists by name.
///
/// Empty while the tree isn't built and between files. Files in flight past the
/// cap count into `moreActiveFiles`, a floor (`≥`) when the window stopped
/// before the tree's end and more could sit below it.
fn active_folders_yaml(tree: &TransferTreeView) -> String {
    if !tree.ready {
        return String::new();
    }
    // The open folder rows above the current row, outermost first: index is depth.
    let mut folders: Vec<&TransferTreeRow> = tree.ancestors.iter().map(|ancestor| &ancestor.row).collect();
    // Per folder holding active files (`None` for the top level): its folder
    // chain and its listed files, in tree order.
    let mut groups: Vec<(Vec<&TransferTreeRow>, Vec<&TransferTreeRow>)> = Vec::new();
    let mut listed = 0;
    let mut unlisted = 0;
    for row in &tree.rows {
        folders.truncate(usize::from(row.depth));
        if row.kind == TreeRowKind::Folder {
            folders.push(row);
            continue;
        }
        if row.state != TreeRowState::Active {
            continue;
        }
        if listed == MAX_ACTIVE_FILES {
            unlisted += 1;
            continue;
        }
        listed += 1;
        let holder = folders.last().map(|folder| folder.id);
        match groups
            .iter_mut()
            .find(|(chain, _)| chain.last().map(|folder| folder.id) == holder)
        {
            Some((_, files)) => files.push(row),
            None => groups.push((folders.clone(), vec![row])),
        }
    }
    if groups.is_empty() {
        return String::new();
    }

    let mut yaml = String::from("    activeFolders:\n");
    for (chain, files) in &groups {
        match chain.last() {
            Some(holder) => {
                let path: Vec<&str> = chain.iter().map(|folder| folder.label.as_str()).collect();
                yaml.push_str(&format!("      - folder: {:?}\n", path.join("/")));
                if let Some(counts) = counts_line(
                    holder.bytes_done,
                    holder.bytes_total,
                    holder.files_done,
                    holder.files_total,
                ) {
                    yaml.push_str(&format!("        progress: {counts}\n"));
                }
            }
            // The operation's own `progress:` already speaks for the top level.
            None => yaml.push_str("      - folder: (top level)\n"),
        }
        yaml.push_str("        files:\n");
        for file in files {
            yaml.push_str(&format!("          - {:?}", file.label));
            if file.bytes_total > 0 {
                yaml.push_str(&format!(
                    " ({} / {})",
                    format_size(file.bytes_done),
                    format_size(file.bytes_total)
                ));
            }
            yaml.push('\n');
        }
    }
    if unlisted > 0 {
        let window_end = tree.offset + tree.rows.len() as u32;
        let floor = if window_end < tree.total_rows { "≥" } else { "" };
        yaml.push_str(&format!("    moreActiveFiles: {floor}{unlisted}\n"));
    }
    yaml
}

/// The failure's typed variant name (`source_not_found`, `insufficient_space`,
/// `permission_denied`), taken from its own serde tag so a renamed variant can't
/// leave a hand-written string behind.
///
/// The TAG only, deliberately: the payload's paths already ride the row as
/// `source:` / `destination:`, and its `message` fields carry raw OS text that
/// would need its own redaction pass. What an agent decides on is the class.
fn error_line(error: &cmdr_operations::WriteOperationError) -> String {
    serde_json::to_value(error)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Serde-rendered enum name (`copy`, `rolling_back`), lowercased as a fallback
/// for enums without a `rename_all` attribute.
fn enum_str(value: Option<serde_json::Value>) -> String {
    value
        .as_ref()
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}
