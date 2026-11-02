//! The authored tool table: one entry per tool, for both callers, expanded by `mcp_tools!`.
//!
//! Data, not mechanism. The macro that turns these entries into `get_all_tools`, `execute_tool`,
//! `ToolId`, `tool_access`, and `tool_schema` lives in `registry.rs`, along with the entry form and the
//! handler-shape tags. This file only grows when a tool is added, and the order here is the wire order
//! (`tool_snapshot_tests` pins the bytes). Who may run what is `policy::admit`'s, from `access` alone.

use serde_json::Value;

use super::actions::{ToolError, ToolResult};
use super::actions::{
    app, async_tools, conflicts, cpu, dialog_confirm, dialogs, downloads, eject, favorites, file_ops, image_facts,
    indexing, memory, multi_rename, nav, operation_log, photos, queue, quit, search, tags, unlock, view,
};
use super::ask_plans;
use super::policy::ToolCtx;
use super::registry::{Access, Load, SubjectSpec, Tool, validate_params};
use super::schemas;

mcp_tools! {
    // ── Navigation ──────────────────────────────────────────────────────────
    "select_volume" => {
        id: SelectVolume,
        brief: "Switch a pane to another volume, such as a disk, share, or phone.",
        desc: "Switch a pane to a volume from cmdr://state, preferably by volumeId (names can collide). The pane reopens the folder last used there; the tool replies once it rests. To move within the current volume, use nav_to_path.",
        schema: schemas::select_volume_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Deferred,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_path" => {
        id: NavToPath,
        brief: "Take a pane to a folder by path.",
        desc: "Navigate a pane to a path: absolute, ~-relative, or mtp:// (smb:// is not navigable; reach a share with select_volume). Prefer this over nav_to_parent when you know the target. Archive paths are transparent, so foo.zip/inner navigates inside the archive.",
        schema: schemas::nav_to_path_schema(),
        subject: SubjectSpec::Path("path"),
        access: Access::AppState,
        load: Load::Core,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_parent" => {
        id: NavToParent,
        brief: "Take the focused pane up to its parent folder.",
        desc: "Navigate the focused pane up to its parent folder.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: nav nav::execute_nav_command
    },
    "nav_back" => {
        id: NavBack,
        brief: "Go back in the focused pane's history.",
        desc: "Go back to the focused pane's previous folder in its navigation history.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: nav nav::execute_nav_command
    },
    "nav_forward" => {
        id: NavForward,
        brief: "Go forward in the focused pane's history.",
        desc: "Go forward again (undo a nav_back) in the focused pane's navigation history.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: nav nav::execute_nav_command
    },
    "scroll_to" => {
        id: ScrollTo,
        brief: "Load the rows around an index in a large folder.",
        desc: "Load the file window around an index in a large (paginated) directory so those rows appear in cmdr://state. Needed before move_cursor / select can reach a row outside the currently loaded range.",
        schema: schemas::scroll_to_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Deferred,
        run: nav_params nav::execute_nav_command_with_params
    },

    // ── Cursor ──────────────────────────────────────────────────────────────
    "move_cursor" => {
        id: MoveCursor,
        brief: "Move a pane's cursor to a row, by index or file name.",
        desc: "Focus a pane and move its cursor to a row, by zero-based index or by filename (give one). Flushes pane state, so a following copy / move / delete / rename acts on this row. A missing filename or out-of-range index is an honest error, never a silent no-op.",
        schema: schemas::move_cursor_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Deferred,
        run: nav_params nav::execute_nav_command_with_params
    },
    "open_under_cursor" => {
        id: OpenUnderCursor,
        brief: "Open the item under the cursor, like pressing Enter.",
        desc: "Open the item under the cursor, like pressing Enter: enter a folder, open a file, or connect a network host / share. A person confirms a file or app first; a folder just opens.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::Sensitive,
        load: Load::Deferred,
        ask: ask_plans::plan_open_under_cursor,
        run: ctx_params nav::execute_open_under_cursor
    },

    // ── Selection ───────────────────────────────────────────────────────────
    "select" => {
        id: Select,
        brief: "Select files in a pane by name, by index range, or all of them.",
        desc: "Select files in a pane by names, by an index range (start + count), or all; count=0 clears. Focuses the pane and flushes state, so a following copy / move / delete / compress acts on this selection. names errors if any name isn't in the listing.",
        schema: schemas::select_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Core,
        run: app_params file_ops::execute_select_command
    },

    // ── File operations ─────────────────────────────────────────────────────
    "copy" => {
        id: Copy,
        brief: "Open the copy dialog for a pane's selection, for a person to confirm.",
        desc: "Open the dialog that copies a pane's selection into the folder the other pane shows; already there duplicates each as name (1). A person confirms it, so nothing has happened yet when this returns. Refuses with nothing selected. onConflict presets the dialog's choice for clashes.",
        schema: schemas::copy_schema(),
        subject: SubjectSpec::Pane,
        access: Access::Initiate,
        load: Load::Core,
        run: ctx_params file_ops::execute_copy
    },
    "move" => {
        id: Move,
        brief: "Open the move dialog for a pane's selection, for a person to confirm.",
        desc: "Open the dialog that moves a pane's selection to the other pane. A person confirms it, so nothing has happened yet when this returns. Refuses with nothing selected. onConflict presets the dialog's choice for clashes.",
        schema: schemas::move_schema(),
        subject: SubjectSpec::Pane,
        access: Access::Initiate,
        load: Load::Core,
        run: ctx_params file_ops::execute_move
    },
    "compress" => {
        id: Compress,
        brief: "Open the dialog that zips the selection, for a person to confirm.",
        desc: "Open the dialog that zips a pane's selection (else its cursor item) into a new archive in the other pane. A person confirms it, so nothing has happened yet when this returns.",
        schema: schemas::compress_schema(),
        subject: SubjectSpec::Pane,
        access: Access::Initiate,
        load: Load::Deferred,
        run: ctx_params file_ops::execute_compress
    },
    "delete" => {
        id: Delete,
        brief: "Open the delete dialog for a pane's selection or cursor item, for a person to confirm.",
        desc: "Open the dialog that deletes a pane's selection (else its cursor item). A person confirms it, so nothing has happened yet when this returns. mode presets trash vs permanent; omit for the pane's default.",
        schema: schemas::delete_schema(),
        subject: SubjectSpec::Pane,
        access: Access::Initiate,
        load: Load::Core,
        run: ctx_params file_ops::execute_delete
    },
    "rename" => {
        id: Rename,
        brief: "Open the inline rename editor on an item, for a person to confirm.",
        desc: "Open the inline rename editor on an item (the named item, else the cursor item) in a pane, \
               prefilled with newName. A person confirms it, so nothing has happened yet when this returns.",
        schema: schemas::rename_schema(),
        subject: SubjectSpec::Pane,
        access: Access::Initiate,
        load: Load::Core,
        run: ctx_params file_ops::execute_rename
    },
    // The Multi-Rename Tool (⌃M) for an agent: `Propose`, never a write. A dry run previews; a
    // real run stages the ready rows as one rename group on the suggested-ops spine, which the
    // user approves or rejects in Cmdr. See `actions/multi_rename.rs`.
    "multi_rename" => {
        id: MultiRename,
        brief: "Preview a pattern rename of a pane's files, or stage it for a person to review.",
        desc: "Pattern-rename the pane's files with the Multi-Rename Tool's settings (masks, search and replace, case, counter; every one optional). dryRun (default) previews each row's new name and status, paged with start/limit. dryRun false stages every ready row as ONE rename proposal for the user to review and approve in Cmdr; it never renames anything itself.",
        schema: schemas::multi_rename_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Propose,
        load: Load::Deferred,
        run: ctx_params multi_rename::execute_multi_rename
    },
    "mkdir" => {
        id: Mkdir,
        brief: "Create a folder by name, or open the naming dialog when you give no name.",
        desc: "Create a folder in a pane. A name creates it directly (errors on a name conflict); no name opens the \
               naming dialog for a person.",
        schema: schemas::mkdir_schema(),
        subject: SubjectSpec::Pane,
        access: Access::LightWrite,
        load: Load::Core,
        run: ctx_params file_ops::execute_mkdir
    },
    "mkfile" => {
        id: Mkfile,
        brief: "Create an empty file by name, or open the naming dialog when you give no name.",
        desc: "Create an empty file in a pane. A name creates it directly (errors on a name conflict); no name opens \
               the naming dialog for a person.",
        schema: schemas::mkfile_schema(),
        subject: SubjectSpec::Pane,
        access: Access::LightWrite,
        load: Load::Deferred,
        run: ctx_params file_ops::execute_mkfile
    },
    "refresh" => {
        id: Refresh,
        brief: "Re-read the focused pane's listing after an outside change.",
        desc: "Force a re-read of the focused pane's listing (from disk on local volumes; the watcher cache short-circuits on MTP / SMB). Use after an out-of-band change; navigation and file ops already refresh on their own.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: app_only file_ops::execute_refresh
    },
    "tag" => {
        id: Tag,
        brief: "Set, toggle, or clear macOS Finder color tags on files. Tags have no undo.",
        desc: "Set macOS Finder color tags on files by name (else selection, else cursor). set: \
               make the colors exactly (keeps colorless tags). toggle: flip each color. clear: \
               remove all. macOS only; tags show in cmdr://state as [tags:red,blue].",
        schema: schemas::tag_schema(),
        subject: SubjectSpec::Pane,
        access: Access::LightWrite,
        load: Load::Deferred,
        run: app_params tags::execute_tag
    },

    // ── View ────────────────────────────────────────────────────────────────
    "toggle_hidden" => {
        id: ToggleHidden,
        brief: "Show or hide hidden files in the file lists.",
        desc: "Toggle whether hidden (dotfile) files show in the file lists (the showHidden flag in cmdr://state).",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: app_only view::execute_toggle_hidden
    },
    "set_view_mode" => {
        id: SetViewMode,
        brief: "Set a pane's view mode to brief or full.",
        desc: "Set a pane's view mode: brief (names, only the cursor row detailed) or full (size and date on every row). full makes cmdr://state carry those details for all rows, not just the cursor.",
        schema: schemas::set_view_mode_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Deferred,
        run: app_params view::execute_set_view_mode
    },
    "sort" => {
        id: Sort,
        brief: "Sort a pane by name, extension, size, or date.",
        desc: "Sort a pane by a field (name, ext, size, modified, created) and order (asc / desc).",
        schema: schemas::sort_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Deferred,
        run: app_params view::execute_sort
    },

    // ── Tabs ────────────────────────────────────────────────────────────────
    "tab" => {
        id: Tab,
        brief: "Open, close, activate, pin, reopen, or move a pane's tabs.",
        desc: "Manage a pane's tabs: new, close, close_others, activate, set_pinned, reopen (restore the last-closed tab), or move (reorder with toIndex, or send to the other pane with toPane). tabId defaults to the active tab where it applies; see each pane's tabs in cmdr://state.",
        schema: schemas::tab_schema(),
        subject: SubjectSpec::Pane,
        access: Access::AppState,
        load: Load::Deferred,
        run: app_params app::execute_tab
    },

    // ── Dialogs ─────────────────────────────────────────────────────────────
    "dialog" => {
        id: Dialog,
        brief: "Open, focus, or close a dialog.",
        desc: "Open, focus, or close a dialog. Open/focus: settings, file-viewer, about, onboarding. Close: any id from cmdr://dialogs/available. cmdr://state lists what's open.",
        schema: schemas::dialog_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Core,
        run: ctx_params dialogs::execute_dialog_command
    },
    "dialog_confirm" => {
        id: DialogConfirm,
        brief: "Confirm an open dialog or the inline rename editor.",
        desc: "Press Confirm on an open dialog, or commit the inline rename editor, in a person's place. cmdr://state lists what's open.",
        schema: schemas::dialog_confirm_schema(),
        subject: SubjectSpec::None,
        access: Access::DevOnly("the one way past a person; it IS the human step"),
        load: Load::Deferred,
        run: app_params dialog_confirm::execute_dialog_confirm
    },
    "open_search_dialog" => {
        id: OpenSearchDialog,
        brief: "Open the search dialog with a prefilled query and filters.",
        desc: "Open the search dialog with optional pre-filled query and filters. If autoRun (default true), runs the search immediately. Acks once the dialog has mounted; does not wait for results to render.",
        schema: schemas::open_search_dialog_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: ctx_params dialogs::execute_open_search_dialog
    },

    // ── App ─────────────────────────────────────────────────────────────────
    "quit" => {
        id: Quit,
        brief: "Quit Cmdr, through the gate Cmd-Q uses.",
        desc: "Quit Cmdr, via the gate Cmd-Q uses. Nothing running: outcome 'quitting'. A copy, move, or delete still going: it does NOT quit; 'held' names them and the ms left, and a person answers quit-confirmation or the countdown quits anyway. A person confirms first.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::Sensitive,
        load: Load::Deferred,
        ask: ask_plans::plan_quit,
        run: sync_app quit::execute_quit
    },
    "switch_pane" => {
        id: SwitchPane,
        brief: "Move focus to the other pane.",
        desc: "Toggle focus to the other pane. Takes no parameters. To focus a SPECIFIC pane, use select (with count 0 to clear) or select_volume / nav_to_path on that pane, which focus it.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: sync_app app::execute_switch_pane
    },
    "swap_panes" => {
        id: SwapPanes,
        brief: "Swap what the left and right panes show.",
        desc: "Swap left and right pane directories, view modes, sort orders, and selections",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: sync_app app::execute_swap_panes
    },

    // ── Search ──────────────────────────────────────────────────────────────
    // The only tool either caller has for finding a file by name, and both read the same typed result.
    "search" => {
        id: Search,
        brief: "Find files and folders across one drive by name, size, date, or type.",
        desc: "Find files and folders across ONE whole drive by name pattern, size, date, or type. list_dir ranks one folder's children instead. Reads the index and walks the rest, so an unindexed drive still answers, only slower. Names and metadata only: inspect_file reads contents, and a date is when a file last CHANGED, never saved or opened. Disk space: sortBy size with excludeSystemDirs false. No paging, so narrow instead. Cover the drive the question is about and say which one you covered.",
        schema: schemas::search_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Core,
        run: params_only search::execute_search
    },
    "ai_search" => {
        id: AiSearch,
        brief: "Search one drive with a natural-language query, through the configured LLM.",
        desc: "Search with a natural-language query; the configured LLM turns it into a structured search over one drive, reading the index and walking whatever it hasn't covered. Use search instead when you can express the query as a pattern or filter (no LLM call).",
        schema: schemas::ai_search_schema(),
        subject: SubjectSpec::None,
        access: Access::DevOnly("the agent is already an LLM; a nested call bills twice and hides the translation"),
        load: Load::Deferred,
        run: app_params search::execute_ai_search
    },

    // ── Settings ────────────────────────────────────────────────────────────
    "set_setting" => {
        id: SetSetting,
        brief: "Change a setting. See cmdr://settings for what exists.",
        desc: "Set a setting value. Use the cmdr://settings resource to discover available settings and their constraints. \
               A setting marked mcpSettable: false there is refused; one marked mcpSettable: offOnly accepts only false. \
               A person confirms the change first, seeing the old and new value, unless the setting is marked safe.",
        schema: schemas::set_setting_schema(),
        subject: SubjectSpec::Setting,
        access: Access::Sensitive,
        load: Load::Deferred,
        ask: ask_plans::plan_set_setting,
        run: app_params async_tools::execute_set_setting
    },

    // ── Indexing ────────────────────────────────────────────────────────────
    "indexing" => {
        id: Indexing,
        brief: "Turn a volume's drive indexing on or off, rescan it, or forget it.",
        desc: "Control one volume's drive indexing. Actions: enable (on, starts first scan), \
               disable (off, keeps DB), rescan (fresh full scan), forget (delete DB). enable/rescan \
               return once scanning starts; poll await index_status fresh for done. See cmdr://indexing. A person confirms first.",
        schema: schemas::indexing_schema(),
        subject: SubjectSpec::Volume("volumeId"),
        access: Access::Sensitive,
        load: Load::Deferred,
        ask: ask_plans::plan_indexing,
        run: params_only indexing::execute_indexing
    },

    // ── Queue ───────────────────────────────────────────────────────────────
    "queue" => {
        id: Queue,
        brief: "Pause, resume, or cancel running operations, or answer what they wait on.",
        desc: "Control the operation queue: pause / resume / cancel one operationId, or \
               pause_all / resume_all. cancel keeps what's written, and also takes operationIds \
               (array) for several; operations_rollback asks a person to undo it instead. \
               continue_without_gone answers a pendingSourcesGone question; skip_locked answers a \
               pendingLockedSources one. See cmdr://state operations for ids.",
        schema: schemas::queue_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: params_only queue::execute_queue
    },
    "queue_move_locked" => {
        id: QueueMoveLocked,
        brief: "Answer a move's Locked items question by unlocking them for the move.",
        desc: "Answer a move's pendingLockedSources question in a person's place: unlock its Locked items for the move, and lock each again where it lands. See cmdr://state operations for ids.",
        schema: schemas::queue_move_locked_schema(),
        subject: SubjectSpec::Operation("operationId"),
        access: Access::DevOnly("the question is already in front of the person, and it clears their Locked flag or deny ACL"),
        load: Load::Deferred,
        run: params_only queue::execute_queue_move_locked
    },
    "resolve_conflict" => {
        id: ResolveConflict,
        brief: "Answer one name clash a running operation is parked on.",
        desc: "Answer ONE name clash a running operation is parked on: skip / overwrite / rename that file, \
               applyToAll for the rest. Read cmdr://state operations first for the pendingConflict block. \
               Returns a typed outcome; refuses rather than pretending.",
        schema: schemas::resolve_conflict_schema(),
        subject: SubjectSpec::Operation("operationId"),
        access: Access::DevOnly("the question is already in front of the person"),
        load: Load::Deferred,
        run: params_only conflicts::execute_resolve_conflict
    },
    "unlock" => {
        id: Unlock,
        brief: "Answer the password prompt of a locked archive or backup repo.",
        desc: "Answer the password prompt for a locked archive or backup repo in cmdr://state, naming its path. \
               A repo checks the password first: wrong_password means nothing was kept. \
               browse mode: it re-lists and you're in. transfer mode: it hands over the password ONLY, since \
               supplying one never starts a write; run copy or move again to extract.",
        schema: schemas::unlock_schema(),
        subject: SubjectSpec::Path("path"),
        access: Access::AppState,
        load: Load::Deferred,
        run: app_params unlock::execute_unlock
    },

    // ── Favorites ───────────────────────────────────────────────────────────
    "favorites" => {
        id: Favorites,
        brief: "Add, rename, remove, or reorder the user's favorites.",
        desc: "Manage the user's favorites (the bookmarked folders the favorites menu lists). \
               add: path (+ optional name). rename: id + name. remove: id. reorder: orderedIds, \
               the COMPLETE new ordering. Discover ids in cmdr://state favorites.",
        schema: schemas::favorites_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: params_only favorites::execute_favorites
    },

    // ── Network ─────────────────────────────────────────────────────────────
    "connect_to_server" => {
        id: ConnectToServer,
        brief: "Add a manual SMB server by address.",
        desc: "Add a manual SMB server by address. Checks TCP reachability then adds to the host list.",
        schema: schemas::connect_to_server_schema(),
        subject: SubjectSpec::None,
        access: Access::DevOnly("a model-chosen address would be an exfiltration channel; the agent has propose_server"),
        load: Load::Deferred,
        run: app_params async_tools::execute_connect_to_server
    },
    "remove_manual_server" => {
        id: RemoveManualServer,
        brief: "Remove a manually added server from the host list.",
        desc: "Remove a manually-added server from the host list. A person confirms first.",
        schema: schemas::remove_manual_server_schema(),
        subject: SubjectSpec::None,
        access: Access::Sensitive,
        load: Load::Deferred,
        ask: ask_plans::plan_remove_manual_server,
        run: sync_app_params async_tools::execute_remove_manual_server
    },
    "upgrade_smb_to_direct" => {
        id: UpgradeSmbToDirect,
        brief: "Move a mounted SMB volume to a faster direct session.",
        desc: "Upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O. Uses \
               Keychain creds. Returns OK or NeedsCredentials; a server it can't reach, a \
               volume that's gone, or one that isn't an SMB mount comes back as an error. See \
               cmdr://state volumes for each SMB share's connectionState.",
        schema: schemas::upgrade_smb_to_direct_schema(),
        subject: SubjectSpec::Volume("volumeId"),
        access: Access::Session,
        load: Load::Deferred,
        run: app_params async_tools::execute_upgrade_smb_to_direct
    },
    "eject" => {
        id: Eject,
        brief: "Eject a disk, unmount a share, or disconnect a phone or server.",
        desc: "Detach a volume by id: eject a disk or disk image, unmount an SMB share, disconnect \
               a phone, or close a connection to an SFTP or WebDAV server. The server stays saved. \
               A volume of a disk that keeps others mounted goes alone, so that disk stays attached \
               unless scope is disk. Answers which teardown ran; refuses (typed outcome in the error data) while an operation \
               is using the volume, and for a volume that can't be detached at all. See cmdr://state volumes for ids. A person confirms first.",
        schema: schemas::eject_schema(),
        subject: SubjectSpec::Volume("volumeId"),
        access: Access::Sensitive,
        load: Load::Deferred,
        ask: ask_plans::plan_eject,
        run: params_only eject::execute_eject
    },

    // ── Async ───────────────────────────────────────────────────────────────
    "await" => {
        id: Await,
        brief: "Wait for a pane, index, or operation condition to be met.",
        desc: "Wait until a condition is met, after fire-and-forget actions or async events. Pane conditions watch a pane; index_status watches a volume's indexing freshness; operation_complete / operations_idle watch the write-operation queue.",
        schema: schemas::await_schema(),
        subject: SubjectSpec::None,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params async_tools::execute_await
    },

    // ── Downloads ───────────────────────────────────────────────────────────
    "go_to_latest_download" => {
        id: GoToLatestDownload,
        brief: "Take the focused pane to the newest file in ~/Downloads.",
        desc: "Navigate the focused pane to the most recently observed eligible file in ~/Downloads and select it. Errors if no eligible file exists or Cmdr lacks Full Disk Access.",
        schema: schemas::no_params_schema(),
        subject: SubjectSpec::None,
        access: Access::AppState,
        load: Load::Deferred,
        run: app_only downloads::execute_go_to_latest_download
    },

    // ── Operation log ─────────────────────────────────────────────────────────
    "operations_list" => {
        id: OperationsList,
        brief: "List past file operations, newest first, with filters.",
        desc: "List past operations (copy, move, delete, trash, rename, create, compress), newest first; filter by time, item name, kind, initiator, status; paged. In-flight ops: cmdr://state operations and the queue tool.",
        schema: schemas::operations_list_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params operation_log::execute_operations_list
    },
    "operations_get" => {
        id: OperationsGet,
        brief: "Read one past operation's header and a page of its items.",
        desc: "One operation's header plus a page of its item rows (source/dest paths, per-item outcome). Poll it to watch a rollback settle (rollbackState leaves 'rollingBack').",
        schema: schemas::operations_get_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params operation_log::execute_operations_get
    },
    "operations_rollback" => {
        id: OperationsRollback,
        brief: "Open the rollback dialog for an operation, for a person to confirm.",
        desc: "Ask a person to reverse an operation, finished or running: opens the rollback dialog, or refuses with a typed reason and no dialog when it can't roll back. A reversal deletes the copies, moves back, or restores from trash; it rechecks each item, never overwrites, and skips a drifted or occupied item. Answers awaitingPerson, and nothing has happened yet: once the dialog leaves cmdr://state dialogs, read rollbackState with operations_get.",
        schema: schemas::operations_rollback_schema(),
        subject: SubjectSpec::Operation("operationId"),
        access: Access::Initiate,
        load: Load::Deferred,
        run: ctx_params operation_log::execute_operations_rollback
    },

    // ── Photo search ──────────────────────────────────────────────────────────
    // It only reads the media index, and its handler shapes the `media_index` read API and never
    // emits image bytes (text-only DTO). PRIVACY: paths + the in-image OCR snippet / tag it returns are
    // image-derived text that egresses to the agent's provider: `actions/photos.rs`.
    "search_photos" => {
        id: SearchPhotos,
        brief: "Find photos by scene description, text inside the image, or tag.",
        desc: "Find photos by content: a scene description, text inside the image (OCR), or a tag, from the on-device index (no uploads). Returns matching paths plus a short reason. Omit mode to combine description + OCR. Needs image indexing on.",
        schema: photos::search_photos_schema(),
        subject: SubjectSpec::Custom,
        access: Access::ReadContents,
        load: Load::Deferred,
        run: app_params photos::execute_search_photos
    },
    // The LOOKUP direction of the same index (`search_photos` is the query direction): the
    // caller already has the paths and needs to know what's IN each image. Same access and gate as
    // its sibling. PRIVACY: it returns the FULL stored OCR text, not a snippet: the most sensitive
    // thing either photo tool emits. See `actions/image_facts.rs`.
    "image_facts" => {
        id: ImageFacts,
        brief: "Read the recognized text and tags the index stored for given images.",
        desc: "What the image index stored for given images: the recognized text (OCR) plus Vision tags, per path, to name or describe files you already know. Up to 200 paths; each answers indexed or notIndexed. Needs image indexing on.",
        schema: image_facts::image_facts_schema(),
        subject: SubjectSpec::Custom,
        access: Access::ReadContents,
        load: Load::Core,
        run: app_params image_facts::execute_image_facts
    },

    // ── Diagnostics ───────────────────────────────────────────────────────────
    // The one tool that answers a question about CMDR ITSELF rather than the user's files.
    // `access: Read` is the honest classification: every field is a byte count, a region count,
    // or a fixed tag name (`commands/memory_diagnostics.rs` § Privacy).
    "memory_diagnostics" => {
        id: MemoryDiagnostics,
        brief: "Show what Cmdr's own process holds in memory right now.",
        desc: "What this Cmdr process is holding right now: the physical footprint, the Rust heap tagged by the global allocator holding it (mimalloc or system) and split into live data and allocator slack, the other malloc zones, SQLite's page-cache slab, and the kernel's VM map folded by tag with a per-tag region-size histogram. The only reading that spans every allocator. macOS only.",
        schema: schemas::memory_diagnostics_schema(),
        subject: SubjectSpec::None,
        access: Access::Read,
        load: Load::Deferred,
        run: params_only memory::execute_memory_diagnostics
    },
    // Its CPU sibling, over `cmdr_fs::process_cpu`. Same `access: Read` story: thread names are
    // set by our code and our dependencies, everything else is a count or a duration.
    "cpu_diagnostics" => {
        id: CpuDiagnostics,
        brief: "Show where Cmdr's own process spent CPU, per thread.",
        desc: "Where this Cmdr process's CPU went over a window, per named thread: each live thread's CPU delta (and threads folded by name), what threads that exited inside the window spent (the process delta minus the live threads'), wakeups from TASK_POWER_INFO, context switches and syscalls, and how the thread count moved. 'measure' (default) blocks for windowSeconds; for longer windows, 'mark' a baseline and read 'sinceMark' later. Percentages are of one core. macOS only.",
        schema: schemas::cpu_diagnostics_schema(),
        subject: SubjectSpec::None,
        access: Access::Read,
        load: Load::Deferred,
        run: params_only cpu::execute_cpu_diagnostics
    },

    // ── Reads, proposals, memory, and connecting ──────────────────────────────
    // Handlers, schemas, and typed result shapes are colocated in `crate::tools::read`,
    // `propose`, `suggestions`, and `memory`.
    "app_state" => {
        id: AppState,
        brief: "Snapshot both panes and the mounted volumes: what the user sees now.",
        desc: "Snapshot the live app state: both panes (folder, cursor item, selection, view mode, sort) and the mounted volumes with index freshness and connectivity: what the user is looking at right now.",
        schema: crate::tools::read::state::app_state_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Core,
        run: app_params crate::tools::read::state::execute_app_state
    },
    // The one tool that reads user files' CONTENTS (bounded: up to 200 paths, a line window
    // of text per file, archive entry names, image header facts, never bytes). Answers
    // "what's in this file?".
    // PRIVACY: the windows egress to the agent's provider under the cloud AI consent gate;
    // see `tools/read/inspect/`.
    "inspect_file" => {
        id: InspectFile,
        brief: "Read inside files: metadata, text windows, PDF text, archive entries, image facts.",
        desc: "Look inside files: metadata, the format the bytes really are, and per kind the content: a line window of text (any encoding), PDF text by page, an archive's entries (a zip, tar, or 7z, or a path inside one), or an image's dimensions and camera data (then image_facts for what's in it). find searches text files and PDFs. Up to 200 paths; every cut is reported.",
        schema: crate::tools::read::inspect::inspect_file_schema(),
        subject: SubjectSpec::Custom,
        access: Access::ReadContents,
        load: Load::Core,
        run: app_params crate::tools::read::inspect::execute_inspect_file
    },
    "propose_rename_plan" => {
        id: ProposeRenamePlan,
        brief: "Stage a same-folder image rename plan for a person to review.",
        desc: "Stage a same-folder image-file rename plan for the user to review; it changes nothing and approves nothing. At most 200 rows.",
        schema: crate::tools::propose::rename::propose_rename_plan_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Propose,
        load: Load::Core,
        run: app_params crate::tools::propose::rename::execute_propose_rename_plan
    },
    "list_dir" => {
        id: ListDir,
        brief: "List a folder's children and recursive size from the drive index.",
        desc: "List a folder's children and recursive size from the drive index. An unindexed folder is read live, without folder sizes; a volume that isn't connected answers from its last index, dated. sortBy size finds where space goes inside ONE folder; search ranks a whole drive.",
        schema: crate::tools::read::listing::list_dir_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Core,
        run: ctx_params crate::tools::read::listing::execute_list_dir
    },
    "list_pane_files" => {
        id: ListPaneFiles,
        brief: "List the focused pane's selection or folder, with its location.",
        desc: "List up to 200 entries of the focused pane from its listing cache, never the index or disk. A selection hides the folder's other rows: scope says which you got. Its location is the volume ID and parent path a rename proposal needs.",
        schema: crate::tools::read::pane_listing::list_pane_files_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Core,
        run: app_params crate::tools::read::pane_listing::execute_list_pane_files
    },
    "important_folders" => {
        id: ImportantFolders,
        brief: "List the most important folders across scored volumes.",
        desc: "List the most important folders across scored volumes, highest first, each a location with its score and why. A row's folded counts near-duplicate subfolders merged into it; pass its location as under + volumeId to list them. Works offline, so an unmounted-but-scored drive still answers.",
        schema: crate::tools::read::importance::important_folders_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params crate::tools::read::importance::execute_important_folders
    },
    "folder_importance" => {
        id: FolderImportance,
        brief: "Explain one folder's importance score.",
        desc: "Explain one folder's importance: scored (0-1 score, signal breakdown, and whether it is stale since the latest scan), floored to zero by design (with the reason), or unscored. Works offline.",
        schema: crate::tools::read::importance::folder_importance_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params crate::tools::read::importance::execute_folder_importance
    },
    "list_suggestions" => {
        id: ListSuggestions,
        brief: "List what Ask Cmdr already proposed, as sweeps and groups with counts.",
        desc: "List the operations Ask Cmdr already proposed, as sweeps and groups with op COUNTS, never the ops. Default status pending: what still waits on the user.",
        schema: crate::tools::suggestions::list_suggestions_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params crate::tools::suggestions::execute_list_suggestions
    },
    "get_suggestion_group" => {
        id: GetSuggestionGroup,
        brief: "Read one proposed group and a page of its files.",
        desc: "Read one proposed group: verb, target, reversibility, and a page of the files it would act on (total / returned / truncated; page with offset). Sizes and dates are what the index held when the group was proposed, not the files now.",
        schema: crate::tools::suggestions::get_suggestion_group_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Deferred,
        run: app_params crate::tools::suggestions::execute_get_suggestion_group
    },
    "propose_suggestions" => {
        id: ProposeSuggestions,
        brief: "Stage file operations for a person to review.",
        desc: "Propose file operations for the user to review. It stages a proposal and changes nothing: only the user approves. Name up to 200 paths per group, or describe thousands with a selector, resolved against the drive index once, now. A whole folder is ONE op: give its path. A place on a volume that isn't connected refuses. sweepId plus groupId replaces a pending group you proposed.",
        schema: crate::tools::suggestions::propose_suggestions_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Propose,
        load: Load::Core,
        run: ctx_params crate::tools::suggestions::execute_propose_suggestions
    },
    "propose_server" => {
        id: ProposeServer,
        brief: "Suggest saving or changing a server, for a person to approve.",
        desc: "Suggest saving a server, or renaming or refoldering a saved one, for the user to approve. It saves and contacts nothing. Never send a password: the user types it when connecting. Only when the user asked.",
        schema: crate::tools::suggestions::propose_server_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Propose,
        load: Load::Deferred,
        run: ctx_params crate::tools::suggestions::execute_propose_server
    },
    "nothing_to_suggest" => {
        id: NothingToSuggest,
        brief: "Say the activity you were shown is not worth raising, and stop.",
        desc: "Say the activity you were shown is not worth telling the user about, and stop: call it instead of proposing when nothing deserves a person's attention. Changes nothing, shows nothing.",
        schema: crate::tools::quiet::nothing_to_suggest_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Core,
        run: app_params crate::tools::quiet::execute_nothing_to_suggest
    },
    "memory_write" => {
        id: MemoryWrite,
        brief: "Save a note about the user in your memory folder.",
        desc: "Save a note about the user in your memory folder, creating the file or replacing it whole: facts about them and how they like things, never instructions to yourself. Use AGENTS.md unless you have a reason not to.",
        schema: crate::tools::memory::memory_write_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Memory,
        load: Load::Core,
        run: app_params crate::tools::memory::execute_memory_write
    },
    "memory_edit" => {
        id: MemoryEdit,
        brief: "Change or drop one part of a memory file.",
        desc: "Change or drop one part of a memory file (oldString must appear exactly once; an empty newString deletes it): prune what went stale instead of rewriting the file.",
        schema: crate::tools::memory::memory_edit_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Memory,
        load: Load::Deferred,
        run: app_params crate::tools::memory::execute_memory_edit
    },
    "list_volumes" => {
        id: ListVolumes,
        brief: "List every volume with its kind, root, index freshness, and connection state.",
        desc: "List every volume (disks, shares, servers, phones) with kind, root (the path its locations start with), index freshness (fresh / scanning / stale / off; a not-connected one is stale, with asOf), and connection state; plus favorites, located when on a connected volume.",
        schema: crate::tools::read::volumes::list_volumes_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Read,
        load: Load::Core,
        run: app_params crate::tools::read::volumes::execute_list_volumes
    },
    "connect_volume" => {
        id: ConnectVolume,
        brief: "Connect a saved server or share with its stored credentials.",
        desc: "Connect a saved server or share with its stored credentials, or wait out the reconnect of one that dropped. A missing password answers needsCredentials.",
        schema: crate::tools::connect::connect_volume_schema(),
        subject: SubjectSpec::Custom,
        access: Access::Session,
        load: Load::Deferred,
        run: ctx_params crate::tools::connect::execute_connect_volume
    },
}
