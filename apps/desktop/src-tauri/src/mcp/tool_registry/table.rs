//! The authored tool table: one entry per MCP tool, expanded by [`mcp_tools!`](super) into
//! every consumer view.
//!
//! Data, not mechanism. The macro that turns these entries into `get_all_tools`,
//! `agent_tool_view`, `execute_tool`, `tool_gate`, `tool_consumers`, `tool_access`, and
//! `tool_schema` lives in `mod.rs`, along with the entry form and the handler-shape tags. This
//! file only grows when a tool is added, and the order here is the wire order
//! (`tool_snapshot_tests` pins the bytes).

use serde_json::Value;

use super::{Access, Consumer, TokenGate, schemas, tool_available_to, validate_params};
use crate::mcp::executor::{ToolError, ToolResult};
use crate::mcp::executor::{
    app, archive_password, async_tools, conflicts, dialogs, downloads, eject, favorites, file_ops, image_facts,
    indexing, nav, operation_log, photos, queue, quit, search, tags, view,
};
use crate::mcp::tools::Tool;

mcp_tools! {
    // ── Navigation ──────────────────────────────────────────────────────────
    "select_volume" => {
        desc: "Switch a pane to a volume by name (as listed in cmdr://state volumes): a disk, SMB share, MTP device, or Network. To move within the current volume, use nav_to_path instead.",
        schema: schemas::select_volume_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_path" => {
        desc: "Navigate a pane to a path: absolute, ~-relative, or mtp:// (smb:// is not navigable; reach a share with select_volume). Prefer this over nav_to_parent when you know the target. Archive paths are transparent, so foo.zip/inner navigates inside the archive.",
        schema: schemas::nav_to_path_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_parent" => {
        desc: "Navigate the focused pane up to its parent folder.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },
    "nav_back" => {
        desc: "Go back to the focused pane's previous folder in its navigation history.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },
    "nav_forward" => {
        desc: "Go forward again (undo a nav_back) in the focused pane's navigation history.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },
    "scroll_to" => {
        desc: "Load the file window around an index in a large (paginated) directory so those rows appear in cmdr://state. Needed before move_cursor / select can reach a row outside the currently loaded range.",
        schema: schemas::scroll_to_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },

    // ── Cursor ──────────────────────────────────────────────────────────────
    "move_cursor" => {
        desc: "Focus a pane and move its cursor to a row, by zero-based index or by filename (give one). Flushes pane state, so a following copy / move / delete / rename acts on this row. A missing filename or out-of-range index is an honest error, never a silent no-op.",
        schema: schemas::move_cursor_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },
    "open_under_cursor" => {
        desc: "Open the item under the cursor, like pressing Enter: enter a folder, open a file, or connect a network host / share.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },

    // ── Selection ───────────────────────────────────────────────────────────
    "select" => {
        desc: "Select files in a pane by names, by an index range (start + count), or all; count=0 clears. Focuses the pane and flushes state, so a following copy / move / delete / compress acts on this selection. names errors if any name isn't in the listing.",
        schema: schemas::select_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_select_command
    },

    // ── File operations ─────────────────────────────────────────────────────
    "copy" => {
        desc: "Copy the selection (else the cursor item) into the folder the other pane shows; already there duplicates each as name (1). Without autoConfirm, opens the confirm dialog. With autoConfirm, starts and returns the operationId. onConflict resolves clashes.",
        schema: schemas::copy_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_copy
    },
    "move" => {
        desc: "Move the selection (else the cursor item) to the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete, or steer with queue). onConflict resolves file clashes.",
        schema: schemas::move_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_move
    },
    "compress" => {
        desc: "Zip the selection into a new archive in the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts and returns the operationId — unless the target archive exists, where the dialog stays open to confirm the overwrite.",
        schema: schemas::compress_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_compress
    },
    "delete" => {
        desc: "Delete the selection (else the cursor item). Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete on it). mode presets trash vs permanent; omit for the pane's default.",
        schema: schemas::delete_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_delete
    },
    "rename" => {
        desc: "Rename an item (the named item, else the cursor item) in a pane. Without autoConfirm, \
               opens the inline rename editor prefilled with newName for the user to confirm. With \
               autoConfirm, renames directly (errors if the name already exists).",
        schema: schemas::rename_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_rename
    },
    "mkdir" => {
        desc: "Create a folder in the focused pane, or pass pane to target the other. No name opens the naming \
               dialog (user confirms, not MCP); a name prefills it; name + autoConfirm creates directly (errors on \
               a name conflict).",
        schema: schemas::mkdir_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_mkdir
    },
    "mkfile" => {
        desc: "Create an empty file in the focused pane, or pass pane to target the other. No name opens the naming \
               dialog (user confirms, not MCP); a name prefills it; name + autoConfirm creates directly (errors on \
               a name conflict).",
        schema: schemas::mkfile_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_mkfile
    },
    "refresh" => {
        desc: "Force a re-read of the focused pane's listing (from disk on local volumes; the watcher cache short-circuits on MTP / SMB). Use after an out-of-band change; navigation and file ops already refresh on their own.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_only file_ops::execute_refresh
    },
    "tag" => {
        desc: "Set macOS Finder color tags on files by name (else selection, else cursor). set: \
               make the colors exactly (keeps colorless tags). toggle: flip each color. clear: \
               remove all. macOS only; tags show in cmdr://state as [tags:red,blue].",
        schema: schemas::tag_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params tags::execute_tag
    },

    // ── View ────────────────────────────────────────────────────────────────
    "toggle_hidden" => {
        desc: "Toggle whether hidden (dotfile) files show in the file lists (the showHidden flag in cmdr://state).",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_only view::execute_toggle_hidden
    },
    "set_view_mode" => {
        desc: "Set a pane's view mode: brief (names, only the cursor row detailed) or full (size and date on every row). full makes cmdr://state carry those details for all rows, not just the cursor.",
        schema: schemas::set_view_mode_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params view::execute_set_view_mode
    },
    "sort" => {
        desc: "Sort a pane by a field (name, ext, size, modified, created) and order (asc / desc).",
        schema: schemas::sort_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params view::execute_sort
    },

    // ── Tabs ────────────────────────────────────────────────────────────────
    "tab" => {
        desc: "Manage a pane's tabs: new, close, close_others, activate, set_pinned, or reopen (restore the last-closed tab). tabId defaults to the active tab where it applies; see each pane's tabs in cmdr://state.",
        schema: schemas::tab_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params app::execute_tab
    },

    // ── Dialogs ─────────────────────────────────────────────────────────────
    "dialog" => {
        desc: "Open, focus, close, or confirm a dialog. Open/focus: settings, file-viewer, about, onboarding. Close: any id from cmdr://dialogs/available. confirm (token-gated) accepts an open confirmation. cmdr://state lists what's open.",
        schema: schemas::dialog_schema(),
        gate: TokenGate::IfConfirmAction,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params dialogs::execute_dialog_command
    },
    "open_search_dialog" => {
        desc: "Open the search dialog with optional pre-filled query and filters. If autoRun (default true), runs the search immediately. Acks once the dialog has mounted; does not wait for results to render.",
        schema: schemas::open_search_dialog_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params dialogs::execute_open_search_dialog
    },

    // ── App ─────────────────────────────────────────────────────────────────
    "quit" => {
        desc: "Quit Cmdr, via the gate Cmd-Q uses. Nothing running: outcome 'quitting'. A copy, move, or delete still going: it does NOT quit; 'held' names them and the ms left. Answer with dialog confirm/close on quit-confirmation, or the countdown quits anyway.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app quit::execute_quit
    },
    "switch_pane" => {
        desc: "Toggle focus to the other pane. Takes no parameters (a pane arg is ignored). To focus a SPECIFIC pane, use select (with count 0 to clear) or select_volume / nav_to_path on that pane, which focus it.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app app::execute_switch_pane
    },
    "swap_panes" => {
        desc: "Swap left and right pane directories, view modes, sort orders, and selections",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app app::execute_swap_panes
    },

    // ── Search ──────────────────────────────────────────────────────────────
    // Shared by the Ask Cmdr agent and external MCP clients: it is the only tool either one
    // has for finding a file by name, and both read the same typed result. `ai_search` stays
    // ai-client-only — the agent is already an LLM holding the user's prose, so a second model
    // call to translate it would bill twice and hide the translation from the caller.
    "search" => {
        desc: "Find files and folders across ONE whole drive by name pattern, size, date, or type. list_dir ranks one folder's children instead. Reads the index and walks the rest, so an unindexed drive still answers, only slower. Names and metadata only: inspect_file reads contents, and a date is when a file last CHANGED, never saved or opened. Disk space: sortBy size with excludeSystemDirs false. No paging, so narrow instead. Cover the drive the question is about and say which one you covered.",
        schema: schemas::search_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: params_only search::execute_search
    },
    "ai_search" => {
        desc: "Search with a natural-language query; the configured LLM turns it into a structured search over one drive, reading the index and walking whatever it hasn't covered. Use search instead when you can express the query as a pattern or filter (no LLM call).",
        schema: schemas::ai_search_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Read,
        run: params_only search::execute_ai_search
    },

    // ── Settings ────────────────────────────────────────────────────────────
    "set_setting" => {
        desc: "Set a setting value. Use the cmdr://settings resource to discover available settings and their constraints.",
        schema: schemas::set_setting_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params async_tools::execute_set_setting
    },

    // ── Indexing ────────────────────────────────────────────────────────────
    "indexing" => {
        desc: "Control one volume's drive indexing. Actions: enable (on, starts first scan), \
               disable (off, keeps DB), rescan (fresh full scan), forget (delete DB). enable/rescan \
               return once scanning starts; poll await index_status fresh for done. See cmdr://indexing.",
        schema: schemas::indexing_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only indexing::execute_indexing
    },

    // ── Queue ───────────────────────────────────────────────────────────────
    "queue" => {
        desc: "Control the operation queue: pause / resume / cancel one operationId, or \
               pause_all / resume_all. cancel also takes operationIds (array) for several; \
               rollback: true deletes already-copied files (single op, token-gated). See \
               cmdr://state operations for ids.",
        schema: schemas::queue_schema(),
        gate: TokenGate::IfRollback,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only queue::execute_queue
    },
    "resolve_conflict" => {
        desc: "Answer ONE name clash a running operation is parked on: skip / overwrite / rename that file, \
               applyToAll for the rest. Read cmdr://state operations first for the pendingConflict block. \
               Returns a typed outcome; refuses rather than pretending. Token-gated.",
        schema: schemas::resolve_conflict_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only conflicts::execute_resolve_conflict
    },
    "unlock_archive" => {
        desc: "Answer the archive-password prompt in cmdr://state, naming its archivePath. \
               browse mode: it re-lists and you're in. transfer mode: it stores the password ONLY, since \
               supplying one never starts a write; run copy or move again to extract. Token-gated.",
        schema: schemas::unlock_archive_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params archive_password::execute_unlock_archive
    },

    // ── Favorites ───────────────────────────────────────────────────────────
    "favorites" => {
        desc: "Manage the user's favorites (the switcher's Favorites section). add: path (+ \
               optional name). rename: id + name. remove: id. reorder: orderedIds, the COMPLETE \
               new ordering. Discover ids in cmdr://state favorites.",
        schema: schemas::favorites_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only favorites::execute_favorites
    },

    // ── Network ─────────────────────────────────────────────────────────────
    "connect_to_server" => {
        desc: "Add a manual SMB server by address. Checks TCP reachability then adds to the host list.",
        schema: schemas::connect_to_server_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params async_tools::execute_connect_to_server
    },
    "remove_manual_server" => {
        desc: "Remove a manually-added server from the host list.",
        schema: schemas::remove_manual_server_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app_params async_tools::execute_remove_manual_server
    },
    "upgrade_smb_to_direct" => {
        desc: "Upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O. Uses \
               Keychain creds. Returns OK, NeedsCredentials, or NetworkError. See \
               cmdr://state volumes for each SMB share's smbConnectionState.",
        schema: schemas::upgrade_smb_to_direct_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params async_tools::execute_upgrade_smb_to_direct
    },
    "eject" => {
        desc: "Eject an ejectable volume by id (disk or MTP). Refuses honestly while an operation \
               is reading from or writing to the volume, and for non-ejectable volumes. See \
               cmdr://state volumes for ids.",
        schema: schemas::eject_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only eject::execute_eject
    },

    // ── Async ───────────────────────────────────────────────────────────────
    "await" => {
        desc: "Wait until a condition is met, after fire-and-forget actions or async events. Pane conditions watch a pane; index_status watches a volume's indexing freshness; operation_complete / operations_idle watch the write-operation queue.",
        schema: schemas::await_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Read,
        run: app_params async_tools::execute_await
    },

    // ── Downloads ───────────────────────────────────────────────────────────
    "go_to_latest_download" => {
        desc: "Navigate the focused pane to the most recently observed eligible file in ~/Downloads and select it. Errors if no eligible file exists or Cmdr lacks Full Disk Access.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_only downloads::execute_go_to_latest_download
    },

    // ── Operation log ─────────────────────────────────────────────────────────
    "operations_list" => {
        desc: "List past operations (copy, move, delete, trash, rename, create, compress), newest first; filter by time, item name, kind, initiator, status; paged. In-flight ops: cmdr://state operations and the queue tool.",
        schema: schemas::operations_list_schema(),
        gate: TokenGate::Open,
        // Shared read: the agent runtime uses the same core (the schemas fit unchanged).
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params operation_log::execute_operations_list
    },
    "operations_get" => {
        desc: "One operation's header plus a page of its item rows (source/dest paths, per-item outcome). Poll it to watch a rollback settle (rollbackState leaves 'rollingBack').",
        schema: schemas::operations_get_schema(),
        gate: TokenGate::Open,
        // Shared read: the agent runtime uses the same core (the schemas fit unchanged).
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params operation_log::execute_operations_get
    },
    "operations_rollback" => {
        desc: "Reverse a logged operation (delete the copies, move back, restore from trash). Rechecks each item and never overwrites; a drifted or occupied item is skipped. Returns after dispatch: poll operations_get until rollbackState leaves 'rollingBack'.",
        schema: schemas::operations_rollback_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params operation_log::execute_operations_rollback
    },

    // ── Photo search ──────────────────────────────────────────────────────────
    // Shared read (agent-spec D49: one authored entry, both consumer views). The in-app
    // Ask Cmdr agent AND external MCP clients search enriched photos. `access: Read` — it
    // only reads the media index. Handler shapes the `media_index` read API and never emits
    // image bytes (text-only DTO). PRIVACY: paths + the in-image OCR snippet / tag it returns
    // are image-derived text that egresses to the agent's provider — see `executor/photos.rs`.
    "search_photos" => {
        desc: "Find photos by content: a scene description, text inside the image (OCR), or a tag, from the on-device index (no uploads). Returns matching paths plus a short reason. Omit mode to combine description + OCR. Needs image indexing on.",
        schema: photos::search_photos_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params photos::execute_search_photos
    },
    // The LOOKUP direction of the same index (`search_photos` is the query direction): the
    // caller already has the paths and needs to know what's IN each image. Same sharing,
    // access, and gate as its sibling. PRIVACY: it returns the FULL stored OCR text, not a
    // snippet — the most sensitive thing either photo tool emits. See
    // `executor/image_facts.rs`.
    "image_facts" => {
        desc: "What the image index stored for given images: the recognized text (OCR) plus Vision tags, per path, to name or describe files you already know. Up to 200 paths; each answers indexed or notIndexed. Needs image indexing on.",
        schema: image_facts::image_facts_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params image_facts::execute_image_facts
    },

    // ── Agent read-only tools ─────────────────────────────────────────────────
    // The Ask Cmdr agent's own read-only surface (agent-spec D49: one authored registry, two
    // consumer views). `consumers: [Agent]`, `access: Read` — filtered out of `get_all_tools()`,
    // so the ai-client wire snapshot is unchanged. Handlers, schemas, and typed result shapes are
    // colocated in `crate::agent::tools::read` (feature-organized). `gate: Open` is inert here (the
    // agent never crosses the MCP auth boundary); it's the honest classification for a read.
    "app_state" => {
        desc: "Snapshot the live app state: both panes (folder, cursor item, selection, view mode, sort) and the mounted volumes with index freshness and connectivity: what the user is looking at right now.",
        schema: crate::agent::tools::read::state::app_state_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::state::execute_app_state
    },
    // The one tool that reads user files' CONTENTS (bounded: up to 200 paths, a line window
    // of text per file, archive entry names, image header facts, never bytes). Answers
    // "what's in this file?".
    // PRIVACY: the windows egress to the agent's provider under the Ask Cmdr consent gate;
    // see `agent/tools/read/inspect/`.
    "inspect_file" => {
        desc: "Look inside files: metadata, the format the bytes really are, and per kind the content: a line window of text (any encoding), PDF text by page, an archive's entries (a zip, tar, or 7z, or a path inside one), or an image's dimensions and camera data (then image_facts for what's in it). find searches text files and PDFs. Up to 200 paths; every cut is reported.",
        schema: crate::agent::tools::read::inspect::inspect_file_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::inspect::execute_inspect_file
    },
    "propose_rename_plan" => {
        desc: "Stage a same-folder image-file rename plan for the user to review; it changes nothing and approves nothing. At most 200 rows.",
        schema: crate::agent::tools::propose::rename::propose_rename_plan_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Propose,
        run: app_params crate::agent::tools::propose::rename::execute_propose_rename_plan
    },
    "list_dir" => {
        desc: "List a folder's children from the drive index (never the disk), plus its recursive size total. Sort by name, size, or modified; page with limit/offset. sortBy size ranks files and folders together by space used, to find where space goes inside ONE folder; search ranks a whole drive.",
        schema: crate::agent::tools::read::listing::list_dir_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::listing::execute_list_dir
    },
    "list_pane_files" => {
        desc: "List up to 200 entries of the focused pane (the selection when one exists, else the folder) from its listing cache, never the index or disk. Returns the volume ID and shared parent path a rename proposal needs.",
        schema: crate::agent::tools::read::pane_listing::list_pane_files_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::pane_listing::execute_list_pane_files
    },
    "important_folders" => {
        desc: "List the most important folders across scored volumes (top-N, or at or above a threshold), highest first, each with its volume and score. Importance is Cmdr's offline signal, so an unmounted-but-scored drive still answers.",
        schema: crate::agent::tools::read::importance::important_folders_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::importance::execute_important_folders
    },
    "folder_importance" => {
        desc: "Explain one folder's importance: scored (0-1 score, signal breakdown, and whether it is stale since the latest scan), floored to zero by design (with the reason), or unscored. Works offline.",
        schema: crate::agent::tools::read::importance::folder_importance_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::importance::execute_folder_importance
    },
    "list_suggestions" => {
        desc: "List the operations Ask Cmdr already proposed, as sweeps and groups with op COUNTS, never the ops. Default status pending: what still waits on the user.",
        schema: crate::agent::tools::suggestions::list_suggestions_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::suggestions::execute_list_suggestions
    },
    "get_suggestion_group" => {
        desc: "Read one proposed group: verb, target, reversibility, and a page of the files it would act on (total / returned / truncated; page with offset). Sizes and dates are what the index held when the group was proposed, not the files now.",
        schema: crate::agent::tools::suggestions::get_suggestion_group_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::suggestions::execute_get_suggestion_group
    },
    "propose_suggestions" => {
        desc: "Propose file operations (move, copy, trash, delete, rename, compress, extract) for the user to review, in groups each approved or rejected on its own. It stages a proposal and changes nothing: only the user approves. Name up to 200 paths per group, or describe thousands with a selector, resolved against the drive index once, now. A whole folder is ONE op: give its path. sweepId plus groupId replaces a pending group you proposed earlier.",
        schema: crate::agent::tools::suggestions::propose_suggestions_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Propose,
        run: app_params crate::agent::tools::suggestions::execute_propose_suggestions
    },
    "nothing_to_suggest" => {
        desc: "Say the activity you were shown is not worth telling the user about, and stop: call it instead of proposing when nothing deserves a person's attention. Changes nothing, shows nothing.",
        schema: crate::agent::tools::quiet::nothing_to_suggest_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::quiet::execute_nothing_to_suggest
    },
    "memory_write" => {
        desc: "Save a note about the user in your memory folder, creating the file or replacing it whole: facts about them and how they like things, never instructions to yourself. Use AGENTS.md unless you have a reason not to.",
        schema: crate::agent::tools::memory::memory_write_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Memory,
        run: app_params crate::agent::tools::memory::execute_memory_write
    },
    "memory_edit" => {
        desc: "Change or drop one part of a memory file (oldString must appear exactly once; an empty newString deletes it): prune what went stale instead of rewriting the file.",
        schema: crate::agent::tools::memory::memory_edit_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Memory,
        run: app_params crate::agent::tools::memory::execute_memory_edit
    },
    "list_volumes" => {
        desc: "List every volume Cmdr can see (local disks, SMB shares, MTP devices, the Network root) with kind, index freshness (fresh / scanning / stale / off), and for SMB its connection state (direct / os_mount / disconnected). mountPath is the path to put in search's scope to cover that drive; no mountPath means nothing can search it.",
        schema: crate::agent::tools::read::volumes::list_volumes_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::volumes::execute_list_volumes
    },
}
