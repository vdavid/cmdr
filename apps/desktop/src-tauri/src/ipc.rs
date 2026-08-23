//! Type-safe IPC: typed Rust↔TS bindings for tauri commands and events via tauri-specta.
//!
//! See [`apps/desktop/src/lib/ipc/CLAUDE.md`](../../../src/lib/ipc/CLAUDE.md) for the
//! frontend side and the migration recipe. The convention is documented in
//! `AGENTS.md` § "Type-safe IPC".
//!
//! ## Why
//!
//! Without typed bindings, Tauri command names are magic strings on both sides: a Rust
//! `#[tauri::command]` plus an `invoke('command_name', args)` on the frontend,
//! with no compile-time link. Renaming the Rust side silently breaks runtime
//! IPC with a generic "not allowed" error. The frontend imports typed
//! `commands.commandName(args)` from generated bindings, so command-name and
//! argument-shape mismatches surface at `pnpm check`.
//!
//! ## How
//!
//! - Each command has `#[tauri::command]` + `#[specta::specta]`.
//! - Each DTO crossing the IPC boundary has `#[derive(specta::Type)]`.
//! - [`builder()`] returns a [`tauri_specta::Builder`] holding every command and event the app
//!   exposes; [`run`](crate::run) attaches it to `tauri::Builder::default()` via
//!   `.invoke_handler(builder.invoke_handler())` and `builder.mount_events(app)` in setup.
//! - In debug builds we call `builder().export(...)` to regenerate
//!   `apps/desktop/src/lib/ipc/bindings.ts` on each launch (that's the only place the bindings are
//!   written to disk; everything else just imports them).
//!
//! ## One manifest, two expansions
//!
//! The two halves of the command surface are collected by macros that can't take the same
//! input: `specta::function::collect_functions!` takes bare paths and rejects `#[cfg(...)]`,
//! while `tauri::generate_handler![]` takes a `#[cfg(...)]` per entry but hands back an
//! opaque invoke handler nothing else can read. Both need the same commands.
//!
//! So `ipc_command_manifest!` IS the list: every command written once, grouped by the
//! `#[cfg]` predicate that decides whether the group compiles, handed to whichever consumer
//! macro asked for it. `build_invoke_handler!` turns it into the runtime dispatch table;
//! `define_type_collectors!` turns it into `collect_all_types`, which feeds
//! `tauri_specta::internal::command` and so the exported `bindings.ts`. A command that
//! reaches one reaches the other, which is what stops `bindings.ts` from advertising a
//! command the invoke handler doesn't answer.
//!
//! Commands specta can't describe (generic over `R: Runtime`, streaming over a `Channel<T>`,
//! or carrying a `serde_json::Value`) sit in a group's `dispatch_only` list: registered for
//! dispatch, absent from the bindings, and reached from the frontend by raw invoke.

use tauri_specta::{Builder, collect_events};

use crate::agent::chat::stream::AskCmdrTurn;
use crate::agent::suggested_ops::SuggestionsChanged;
use crate::agent::wake::{AgentWakeStaged, AgentWakeStatus};
use crate::commands::search::SearchIndexReadyEvent;
use crate::events::index_mapping::{
    AggregationProgressEvent, IndexAggregationCompleteEvent, IndexCoverageBranchEndedEvent,
    IndexCoverageBranchStartedEvent, IndexCoveragePhaseStartedEvent, IndexDirUpdatedEvent, IndexFreshnessChangedEvent,
    IndexMemoryWarningEvent, IndexPhaseChangedEvent, IndexReplayCompleteEvent, IndexReplayProgressEvent,
    IndexRescanNotificationEvent, IndexScanAbortedEvent, IndexScanCompleteEvent, IndexScanProgressEvent,
    IndexScanStartedEvent, MediaEnrichProgressEvent, MediaEnrichTerminalEvent,
};
use crate::file_system::git::watcher::GitStateChangedPayload;
use crate::file_system::listing::streaming::{
    ListingCancelledEvent, ListingCompleteEvent, ListingErrorEvent, ListingOpeningEvent, ListingProgressEvent,
    ListingReadCompleteEvent,
};
use crate::file_system::write_operations::{
    ConflictInfo, DryRunResult, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use crate::file_system::write_operations::{OperationsChanged, VolumesBusyChanged};
use crate::mtp::{
    MtpDeviceConnected, MtpDeviceDisconnected, MtpExclusiveAccessError, MtpPermissionError, MtpPtpcameradRestored,
    MtpPtpcameradSuppressed, MtpStorageRemoved,
};
use crate::network::{
    NetworkDiscoveryStateChanged, NetworkHostContextAction, NetworkHostFound, NetworkHostLost, NetworkHostResolved,
    SmbFellBackToOsMount, VolumeConnectionChanged,
};
use crate::search::live::events::{SearchCancelledEvent, SearchCompleteEvent, SearchErrorEvent, SearchProgressEvent};
use crate::space_poller::{LowDiskSpacePayload, VolumeSpaceChanged};
use crate::volume_broadcast::{VolumeContextAction, VolumeMounted, VolumeUnmounted, VolumesChanged};
// Window-management events: emit_to-targeted window lifecycle.
use crate::window_events::{
    CloseAbout, CloseAllFileViewers, CloseConfirmation, CloseFileViewer, ExecuteCommand, FocusAbout, FocusConfirmation,
    FocusFileViewer, FocusSettings, ForegroundOperation, McpSettingsClose, OpenFileViewer, OpenSettings,
    PersistRestrictedSetting, RevealPath, TabContextAction, ViewerWordWrapToggled,
};
// AI + system/misc events.
use crate::ai::{
    AiExtracting, AiInstallComplete, AiInstalling, AiServerReady, AiStarting, AiVerifying, DownloadProgress,
};
use crate::downloads::global_shortcut::GlobalShortcutFired;
use crate::downloads::watcher::DownloadDetectedEvent;
use crate::error_reporter::auto_dispatcher::ErrorReportAutoSent;
use crate::file_system::listing::DirectoryDiff;
use crate::file_system::watcher::DirectoryDeletedEvent;
use crate::menu::{MediaIndexFolderChoice, MediaIndexFolderExclusion, MenuSort, SettingsChanged, ViewModeChanged};
use crate::quick_look::{QuickLookClosed, QuickLookKeyEvent};
use crate::quit::QuitRequested;
use crate::restricted_paths::RestrictedPathsChangedPayload;
use crate::system_events::{
    AccentColorChanged, DragImageSize, DragModifiers, MenuBarRebuilt, OsLocalesChanged, ReduceTransparencyChanged,
    SessionCompleteEvent, SessionStartedEvent, SystemTextSizeChanged,
};

/// Public greeting used by the example webview surface; kept here as the
/// foundational smoke test for the specta wiring.
#[tauri::command]
#[specta::specta]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Every IPC command the app exposes, written once, grouped by the `#[cfg]` predicate that
/// decides whether that group compiles. Hands the whole list to a consumer macro; see the
/// module docs for why both consumers have to read the same list.
///
/// Inside a group:
///
/// - `typed:` commands specta can describe. They reach the runtime dispatch table AND
///   `bindings.ts`. `typed unless cfg(<predicate>)` still dispatches them, but holds them
///   back from the bindings under that predicate.
/// - `dispatch_only:` commands the invoke handler answers but specta can't describe. Each
///   one's comment says why.
///
/// Group order (and order within a group) is the order commands land in `bindings.ts`, so
/// moving a line rewrites the generated file. Add a new command at the end of its list, and
/// regenerate with `pnpm bindings:regen`.
macro_rules! ipc_command_manifest {
    ($consumer:ident) => {
        $consumer! {
            // Every target.
            cfg(all()) {
                typed: [
                    crate::ipc::greet,
                    crate::commands::file_system::list_directory_start,
                    crate::commands::file_system::list_directory_start_streaming,
                    crate::commands::file_system::cancel_listing,
                    crate::commands::file_system::list_directory_end,
                    crate::commands::file_system::refresh_listing,
                    crate::commands::file_system::get_file_range,
                    crate::commands::file_system::get_file_at,
                    crate::commands::file_system::get_file_beside,
                    crate::commands::file_system::get_files_at_indices,
                    crate::commands::file_system::get_paths_at_indices,
                    crate::commands::file_system::get_total_count,
                    crate::commands::file_system::get_brief_column_text_widths,
                    crate::commands::file_system::find_file_index,
                    crate::commands::file_system::find_file_indices,
                    crate::commands::file_system::find_first_fuzzy_match,
                    crate::commands::file_system::resort_listing,
                    crate::commands::file_system::get_path_limits,
                    crate::commands::file_system::enrich_tags,
                    crate::commands::file_system::toggle_tags,
                    crate::commands::file_system::path_exists,
                    crate::commands::file_system::stat_paths_kinds,
                    crate::commands::file_system::create_directory,
                    crate::commands::file_system::create_file,
                    crate::commands::file_system::set_archive_password,
                    crate::commands::file_system::clear_archive_password,
                    crate::commands::file_system::benchmark_log,
                    crate::commands::file_system::copy_files,
                    crate::commands::file_system::move_files,
                    crate::commands::file_system::delete_files,
                    crate::commands::file_system::trash_files,
                    crate::commands::file_system::cancel_write_operation,
                    crate::commands::file_system::cancel_all_write_operations,
                    crate::commands::file_system::start_scan_preview,
                    crate::commands::file_system::cancel_scan_preview,
                    crate::commands::file_system::check_scan_preview_status,
                    crate::commands::file_system::resolve_write_conflict,
                    crate::commands::file_system::list_active_operations,
                    crate::commands::file_system::get_operation_status,
                    crate::commands::file_system::list_operations,
                    crate::commands::file_system::cancel_operation,
                    crate::commands::file_system::cancel_operations,
                    crate::commands::file_system::pause_operation,
                    crate::commands::file_system::resume_operation,
                    crate::commands::file_system::pause_all,
                    crate::commands::file_system::resume_all,
                    crate::commands::file_system::dismiss_failed_operation,
                    crate::commands::file_system::dismiss_all_failed_operations,
                    crate::commands::file_system::copy_between_volumes,
                    crate::commands::file_system::move_between_volumes,
                    crate::commands::file_system::compress_files,
                    crate::commands::file_system::scan_volume_for_copy,
                    crate::commands::file_system::scan_volume_for_conflicts,
                    crate::commands::file_system::get_listing_stats,
                    crate::commands::file_system::refresh_listing_index_sizes,
                    crate::commands::file_system::start_selection_drag,
                    crate::commands::file_system::start_drag_paths,
                    crate::commands::file_system::prepare_self_drag_overlay,
                    crate::commands::file_system::clear_self_drag_overlay,
                    crate::commands::file_system::set_self_drag_resolved_op,
                    crate::commands::file_system::get_git_repo_info,
                    crate::commands::file_system::subscribe_git_state,
                    crate::commands::file_system::unsubscribe_git_state,
                    crate::commands::file_system::get_git_status_for_paths,
                    crate::commands::rename::check_rename_permission,
                    crate::commands::rename::check_rename_validity,
                    crate::commands::rename::rename_file,
                    crate::commands::rename::move_to_trash,
                    crate::commands::restricted_paths::get_restricted_paths,
                    crate::commands::child_window_state::get_child_window_rect,
                    crate::commands::child_window_state::set_child_window_rect,
                    crate::commands::file_viewer::viewer_open,
                    crate::commands::file_viewer::viewer_open_as_text,
                    crate::commands::file_viewer::viewer_get_lines,
                    crate::commands::file_viewer::viewer_get_status,
                    crate::commands::file_viewer::viewer_search_start,
                    crate::commands::file_viewer::viewer_search_poll,
                    crate::commands::file_viewer::viewer_search_cancel,
                    crate::commands::file_viewer::viewer_close,
                    crate::commands::file_viewer::viewer_read_range,
                    crate::commands::file_viewer::viewer_cancel_read,
                    crate::commands::file_viewer::viewer_write_range_to_file,
                    crate::commands::file_viewer::viewer_setup_menu,
                    crate::commands::file_viewer::viewer_set_word_wrap,
                    crate::commands::file_viewer::viewer_get_encoding_options,
                    crate::commands::file_viewer::viewer_set_encoding,
                    crate::commands::file_viewer::viewer_set_tail_mode,
                    crate::commands::file_viewer::viewer_reload,
                    // store_font_metrics is generic (<R: tauri::Runtime>): excluded from specta collection
                    crate::commands::font_metrics::has_font_metrics,
                    crate::commands::icons::get_icons,
                    crate::commands::icons::get_custom_folder_icon_ids,
                    crate::commands::icons::refresh_directory_icons,
                    crate::commands::icons::clear_extension_icon_cache,
                    crate::commands::icons::clear_directory_icon_cache,
                    // These are generic (<R: Runtime>), so specta can't collect them; they stay
                    // in `generate_handler![]` only: `menu::{show_file_context_menu,
                    // show_breadcrumb_context_menu, show_volume_row_context_menu,
                    // show_parent_row_context_menu, update_pin_tab_menu, set_reopen_closed_tab_enabled,
                    // set_file_operations_blocked,
                    // update_menu_context, activate_window_menu, toggle_hidden_files,
                    // sync_menu_show_hidden, update_view_mode_menu, set_ui_language}`,
                    // `window_ordering::{show_main_window, order_window_to_back}`, and
                    // `file_actions::copy_to_clipboard`.
                    crate::commands::menu::show_tab_context_menu,
                    crate::commands::menu::show_network_host_context_menu,
                    crate::commands::file_actions::show_in_finder,
                    crate::commands::quick_look::quick_look_open,
                    crate::commands::quick_look::quick_look_set_path,
                    crate::commands::quick_look::quick_look_close,
                    crate::commands::file_actions::get_info,
                    crate::commands::file_actions::open_in_editor,
                    crate::commands::file_actions::open_path,
                    crate::commands::file_actions::cloud_make_available_offline,
                    crate::commands::file_actions::cloud_remove_download,
                    crate::mcp::pane_state::update_left_pane_state,
                    crate::mcp::pane_state::update_right_pane_state,
                    crate::mcp::pane_state::update_focused_pane,
                    crate::mcp::pane_state::update_pane_tabs,
                    crate::mcp::dialog_state::notify_dialog_opened,
                    crate::mcp::dialog_state::notify_dialog_closed,
                    crate::mcp::dialog_state::register_known_dialogs,
                    crate::commands::sync_status::get_sync_status,
                    crate::commands::smb_diagnostics::list_smb_volumes,
                    crate::commands::smb_diagnostics::get_smb_diagnostics,
                    // `eject_volume` is macOS + Linux only, but the busy set behind it is plain
                    // `crate::file_system`, so this one answers everywhere the invoke handler does.
                    crate::commands::eject::get_busy_volume_ids,
                    crate::volume_broadcast::refresh_volumes,
                    crate::space_poller::watch_volume_space,
                    crate::space_poller::unwatch_volume_space,
                    crate::space_poller::set_disk_space_threshold,
                    crate::space_poller::set_low_disk_space_config,
                    crate::commands::analytics::track_event,
                    crate::commands::beta_signup::beta_signup,
                    crate::commands::crash_reporter::check_pending_crash_report,
                    crate::commands::crash_reporter::dismiss_crash_report,
                    crate::commands::crash_reporter::send_crash_report,
                    crate::commands::error_reporter::send_error_report,
                    // prepare_error_report_preview: BundleManifest contains Breadcrumb.ctx: Option<Value>
                    // which specta can't represent. Excluded; stays in generate_handler![].
                    // record_breadcrumb takes Option<serde_json::Value>: excluded; stays in generate_handler![].
                    crate::commands::error_reporter::record_settings_defaults,
                    crate::commands::feedback::send_feedback,
                    crate::commands::licensing::get_license_status,
                    crate::commands::licensing::activate_license,
                    crate::commands::licensing::verify_license,
                    crate::commands::licensing::commit_license,
                    crate::commands::licensing::get_license_info,
                    crate::commands::licensing::mark_expiration_modal_shown,
                    crate::commands::licensing::mark_commercial_reminder_dismissed,
                    crate::commands::licensing::reset_license,
                    crate::commands::licensing::needs_license_validation,
                    crate::commands::licensing::has_license_been_validated,
                    crate::commands::licensing::validate_license_with_server,
                    crate::ai::manager::get_ai_status,
                    crate::ai::state::get_ai_model_info,
                    crate::ai::manager::get_ai_runtime_status,
                    // configure_ai, start_ai_server, start_ai_download are generic (<R: Runtime>): excluded
                    crate::ai::server::stop_ai_server,
                    crate::ai::connection_check::check_ai_connection,
                    crate::system_memory::get_system_memory_info,
                    crate::system_strings::get_localized_system_strings,
                    crate::intl::get_os_locales,
                    crate::ai::install::cancel_ai_download,
                    crate::ai::install::uninstall_ai,
                    crate::ai::api_keys::save_ai_api_key,
                    crate::ai::api_keys::get_ai_api_key_status,
                    crate::ai::api_keys::delete_ai_api_key,
                    crate::ai::suggestions::get_folder_suggestions,
                    // set_mcp_enabled, set_mcp_port are generic (<R: Runtime>): excluded from specta
                    crate::commands::mcp::get_mcp_running,
                    crate::commands::mcp::get_mcp_port,
                    crate::commands::mcp::get_mcp_token,
                    crate::commands::settings::check_port_available,
                    crate::commands::settings::find_available_port,
                    crate::commands::settings::get_isolated_store_path,
                    crate::commands::settings::update_file_watcher_debounce,
                    crate::commands::settings::update_service_resolve_timeout,
                    crate::commands::settings::update_menu_accelerator,
                    crate::commands::settings::set_direct_smb_connection,
                    crate::commands::settings::set_show_safe_save_files_cmd,
                    crate::commands::settings::set_show_staging_temp_files_cmd,
                    crate::commands::settings::set_smb_concurrency_cmd,
                    crate::commands::settings::set_log_llm_calls,
                    crate::commands::settings::set_image_index_enabled,
                    crate::commands::settings::set_max_log_storage_mb,
                    crate::commands::settings::set_error_reports_enabled,
                    crate::commands::settings::get_restricted_window_settings,
                    crate::commands::settings::persist_restricted_window_setting,
                    crate::commands::settings::set_show_virtual_git_portal,
                    crate::commands::logging::batch_fe_logs,
                    crate::commands::logging::set_log_level,
                    crate::downloads::commands::go_to_latest_download,
                    crate::downloads::commands::downloads_watcher_status,
                    crate::downloads::commands::recheck_downloads_watcher_gate,
                    crate::downloads::commands::set_global_go_to_latest_shortcut,
                    crate::commands::indexing::start_drive_index,
                    crate::commands::indexing::stop_drive_index,
                    crate::commands::indexing::get_index_status,
                    crate::commands::indexing::get_dir_stats,
                    crate::commands::indexing::get_dir_stats_batch,
                    crate::commands::indexing::clear_drive_index,
                    crate::commands::indexing::get_index_disk_usage,
                    crate::commands::indexing::set_indexing_enabled,
                    crate::commands::indexing::start_indexing_after_fda_decision,
                    crate::commands::indexing::get_index_debug_status,
                    crate::commands::indexing::get_volume_index_status,
                    crate::commands::indexing::get_volume_index_status_by_id,
                    crate::commands::indexing::enable_drive_index,
                    crate::commands::indexing::disable_drive_index,
                    crate::commands::indexing::forget_drive_index,
                    crate::commands::indexing::rescan_drive_index,
                    crate::commands::importance::record_visit,
                    crate::commands::media_index::media_index_search_ocr,
                    crate::commands::media_index::media_index_volume_state,
                    crate::commands::media_index::media_index_thumbnail_token,
                    crate::commands::media_index::media_index_drop_thumbnail_tokens,
                    crate::commands::media_index::policy::media_index_set_network_volume_enabled,
                    crate::commands::media_index::policy::media_index_set_always_index_volume,
                    crate::commands::media_index::policy::media_index_set_always_index_folder,
                    crate::commands::media_index::policy::media_index_set_scope,
                    crate::commands::media_index::policy::media_index_set_excluded_folder,
                    crate::commands::media_index::policy::media_index_set_importance_threshold,
                    crate::commands::media_index::policy::media_index_set_parallelism,
                    crate::commands::media_index::policy::media_index_max_parallelism,
                    crate::commands::media_index::policy::media_index_set_semantic_search_enabled,
                    crate::commands::media_index::media_index_covered_count,
                    crate::commands::media_index::media_index_reclaim_preview,
                    crate::commands::media_index::media_index_prune_below_threshold,
                    crate::commands::media_index::media_index_find_similar,
                    crate::commands::media_index::media_index_dedup_clusters,
                    crate::commands::media_index::media_index_search_tag,
                    crate::commands::media_index::media_index_search_semantic,
                    crate::commands::media_index::media_index_clip_model_status,
                    crate::commands::media_index::media_index_download_clip_model,
                    crate::commands::media_index::media_index_delete_clip_model,
                    crate::commands::media_index::media_index_file_status,
                    crate::commands::media_index::media_index_folder_coverage,
                    crate::commands::search::prepare_search_index,
                    crate::commands::search::search_files,
                    crate::commands::search::search_files_streaming,
                    crate::commands::search::cancel_search,
                    crate::commands::search::release_search_index,
                    crate::commands::search::translate_search_query,
                    crate::commands::search::parse_search_scope,
                    crate::commands::search::get_system_dir_excludes,
                    crate::commands::search::get_recent_searches,
                    crate::commands::search::add_recent_search,
                    crate::commands::search::remove_recent_search,
                    crate::commands::search::clear_recent_searches,
                    crate::commands::search::apply_recent_searches_max_count,
                    crate::commands::go_to_path::resolve_go_to_path,
                    crate::commands::go_to_path::get_recent_paths,
                    crate::commands::go_to_path::add_recent_path,
                    crate::commands::go_to_path::remove_recent_path,
                    crate::commands::go_to_path::clear_recent_paths,
                    crate::commands::favorites::add_favorite,
                    crate::commands::favorites::remove_favorite,
                    crate::commands::favorites::rename_favorite,
                    crate::commands::favorites::reorder_favorites,
                    crate::commands::whats_new::get_whats_new,
                    crate::commands::whats_new::whats_new_dev_override,
                    crate::quit::commands::quit_confirm,
                    crate::quit::commands::quit_cancel,
                    crate::commands::operation_log::get_recent_operation_log_entries,
                    crate::commands::operation_log::get_operation_log_detail,
                    crate::commands::operation_log::undo_operations,
                    crate::commands::agent::ask_cmdr_send_message,
                    crate::commands::agent::ask_cmdr_cancel,
                    crate::commands::agent::preflight_bulk_rename,
                    crate::commands::agent::apply_bulk_rename,
                    crate::commands::agent::revise_bulk_rename_row,
                    crate::commands::agent::cancel_bulk_rename_proposal,
                    crate::commands::agent::suggested_ops_list,
                    crate::commands::agent::suggested_ops_page,
                    crate::commands::agent::suggested_ops_reject,
                    crate::commands::agent::suggested_ops_approve,
                    crate::commands::agent::ask_cmdr_record_model_change,
                    crate::commands::agent::ask_cmdr_get_conversation,
                    crate::commands::agent::ask_cmdr_list_conversations,
                    crate::commands::agent::ask_cmdr_search_conversations,
                    crate::commands::agent::ask_cmdr_rename_conversation,
                    crate::commands::agent::ask_cmdr_archive_conversation,
                    crate::commands::agent::ask_cmdr_selection_attachments,
                    crate::commands::agent::ask_cmdr_resolve_attachments,
                    crate::commands::agent::ask_cmdr_consent_status,
                    crate::commands::agent::ask_cmdr_accept_consent,
                    crate::commands::agent::ask_cmdr_revoke_consent,
                    crate::commands::agent::ask_cmdr_conversation_cost,
                    crate::commands::agent::ask_cmdr_cost_summary,
                    crate::commands::agent::ask_cmdr_model_window,
                    crate::commands::agent::ask_cmdr_wake_settings_changed,
                    crate::commands::agent::agent_wake_status,
                    crate::commands::agent::ask_cmdr_memory_folder,
                    crate::commands::agent::ask_cmdr_forget_memory,
                    crate::commands::selection::translate_selection_query,
                    crate::commands::selection::get_recent_selections,
                    crate::commands::selection::add_recent_selection,
                    crate::commands::selection::remove_recent_selection,
                    crate::commands::selection::clear_recent_selections,
                    crate::commands::selection::apply_recent_selections_max_count,
                    crate::commands::e2e::get_e2e_start_path,
                    crate::commands::e2e::is_e2e_mode,
                    crate::commands::e2e::ask_cmdr_fake_active,
                    crate::commands::e2e::is_force_onboarding,
                    crate::commands::clipboard::copy_files_to_clipboard,
                    crate::commands::clipboard::cut_files_to_clipboard,
                    crate::commands::clipboard::copy_paths_to_clipboard,
                    crate::commands::clipboard::cut_paths_to_clipboard,
                    crate::commands::clipboard::read_clipboard_files,
                    crate::commands::clipboard::read_clipboard_text,
                    crate::commands::clipboard::paste_clipboard_as_file,
                    crate::commands::clipboard::clear_clipboard_cut_state,
                ]
                dispatch_only: [
                    // Generic over `R: tauri::Runtime`, which `collect_functions!` can't take.
                    crate::commands::font_metrics::store_font_metrics,
                    crate::commands::font_metrics::extend_font_metrics,
                    crate::commands::menu::show_file_context_menu,
                    crate::commands::menu::show_breadcrumb_context_menu,
                    crate::commands::menu::show_volume_row_context_menu,
                    crate::commands::menu::show_parent_row_context_menu,
                    crate::commands::menu::update_pin_tab_menu,
                    crate::commands::menu::set_reopen_closed_tab_enabled,
                    crate::commands::menu::set_file_operations_blocked,
                    crate::commands::menu::update_menu_context,
                    crate::commands::menu::activate_window_menu,
                    crate::commands::menu::toggle_hidden_files,
                    crate::commands::menu::sync_menu_show_hidden,
                    crate::commands::menu::update_view_mode_menu,
                    crate::commands::menu::set_ui_language,
                    crate::commands::window_ordering::show_main_window,
                    crate::commands::window_ordering::order_window_to_back,
                    crate::commands::file_actions::copy_to_clipboard,
                    crate::commands::mcp::set_mcp_enabled,
                    crate::commands::mcp::set_mcp_port,
                    crate::ai::manager::configure_ai,
                    crate::ai::server::start_ai_server,
                    crate::ai::install::start_ai_download,
                    // Stream over a tauri `Channel<T>`, which specta can't describe, so the frontend
                    // calls these on raw invoke with the documented eslint opt-out.
                    crate::ai::suggestions::stream_folder_suggestions,
                    crate::ai::suggestions::cancel_folder_suggestions,
                    // Carry a `serde_json::Value` (a free-form breadcrumb payload, and a bundle
                    // manifest holding one), which specta can't represent.
                    crate::commands::error_reporter::prepare_error_report_preview,
                    crate::commands::error_reporter::record_breadcrumb,
                ]
            }
            // MTP devices, and the stubs every other target answers with.
            cfg(any(target_os = "macos", target_os = "linux")) {
                typed: [
                    crate::commands::mtp::set_mtp_enabled,
                    crate::commands::mtp::list_mtp_devices,
                    crate::commands::mtp::connect_mtp_device,
                    crate::commands::mtp::get_mtp_device_info,
                    crate::commands::mtp::disconnect_mtp_device,
                    crate::commands::mtp::get_mtp_storages,
                    crate::commands::mtp::list_mtp_directory,
                    crate::commands::mtp::get_ptpcamerad_workaround_command,
                    crate::commands::mtp::delete_mtp_object,
                    crate::commands::mtp::create_mtp_folder,
                    crate::commands::mtp::rename_mtp_object,
                    crate::commands::mtp::move_mtp_object,
                    crate::commands::mtp::scan_mtp_for_copy,
                ]
                dispatch_only: []
            }
            cfg(not(any(target_os = "macos", target_os = "linux"))) {
                typed: [
                    crate::stubs::mtp::set_mtp_enabled,
                    crate::stubs::mtp::list_mtp_devices,
                    crate::stubs::mtp::connect_mtp_device,
                    crate::stubs::mtp::get_mtp_device_info,
                    crate::stubs::mtp::disconnect_mtp_device,
                    crate::stubs::mtp::get_mtp_storages,
                    crate::stubs::mtp::list_mtp_directory,
                    crate::stubs::mtp::get_ptpcamerad_workaround_command,
                    crate::stubs::mtp::delete_mtp_object,
                    crate::stubs::mtp::create_mtp_folder,
                    crate::stubs::mtp::rename_mtp_object,
                    crate::stubs::mtp::move_mtp_object,
                    crate::stubs::mtp::scan_mtp_for_copy,
                ]
                dispatch_only: []
            }
            // The bindings a user runs must not move when a lane turns on a test-only feature: the
            // regen runs while the crate compiles its own tests (that's where `export_bindings_test`
            // writes the file), so holding these back there is what lets every cargo lane share one
            // feature set, and so one `target/`. The E2E specs reach them by raw invoke.
            // `scripts/check/checks/DETAILS.md` § "One feature set across the cargo lanes".
            cfg(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux"))) {
                typed unless cfg(test): [
                    crate::commands::mtp::rescan_virtual_mtp,
                    crate::commands::mtp::pause_virtual_mtp_watcher,
                    crate::commands::mtp::resume_virtual_mtp_watcher,
                ]
                dispatch_only: []
            }
            // Volumes. One block for both platforms: `commands::volumes` is the whole
            // command layer, and the platform difference lives a layer down in
            // `crate::volumes` vs `crate::volumes_linux`, which it picks by `cfg`.
            cfg(any(target_os = "macos", target_os = "linux")) {
                typed: [
                    crate::commands::volumes::list_volumes,
                    crate::commands::volumes::resolve_path_volume,
                    crate::commands::volumes::resolve_location,
                    crate::commands::volumes::get_default_volume_id,
                    crate::commands::volumes::get_volume_space,
                ]
                dispatch_only: []
            }
            cfg(not(any(target_os = "macos", target_os = "linux"))) {
                typed: [
                    crate::stubs::volumes::list_volumes,
                    crate::stubs::volumes::resolve_path_volume,
                    crate::stubs::volumes::resolve_location,
                    crate::stubs::volumes::get_default_volume_id,
                    crate::stubs::volumes::get_volume_space,
                ]
                dispatch_only: []
            }
            // Network hosts and shares.
            cfg(any(target_os = "macos", target_os = "linux")) {
                typed: [
                    crate::commands::network::list_network_hosts,
                    crate::commands::network::resolve_host,
                    crate::commands::network::connect_to_server,
                    crate::commands::network::get_network_discovery_state,
                    crate::commands::network::list_shares_on_host,
                    crate::commands::network::prefetch_shares,
                    crate::commands::network::get_host_auth_mode,
                    crate::commands::network::get_known_shares,
                    crate::commands::network::get_known_share_by_name,
                    crate::commands::network::update_known_share,
                    crate::commands::network::get_username_hints,
                    crate::commands::network::save_smb_credentials,
                    crate::commands::network::get_smb_credentials,
                    crate::commands::network::has_smb_credentials,
                    crate::commands::network::delete_smb_credentials,
                    crate::commands::network::is_using_credential_file_fallback,
                    crate::commands::network::list_shares_with_credentials,
                    crate::commands::network::mount_network_share,
                    crate::commands::network::upgrade_to_smb_volume,
                    crate::commands::network::upgrade_to_smb_volume_with_credentials,
                    crate::commands::network::system_has_saved_smb_password,
                    crate::commands::network::upgrade_to_smb_volume_using_saved_password,
                    crate::commands::network::reconnect_smb_volume,
                    crate::commands::network::reconnect_smb_volume_with_credentials,
                    crate::commands::network::get_volume_sign_in_state,
                    crate::commands::network::disconnect_smb_volume,
                    crate::commands::eject::eject_volume,
                    crate::commands::network::remove_manual_server,
                    crate::commands::network::disconnect_network_host,
                    crate::commands::network::ensure_network_discovery_started,
                    crate::commands::network::set_network_enabled,
                ]
                dispatch_only: []
            }
            // SFTP servers: connecting, host-key trust, secrets, and the server list.
            // ❌ Deliberately no `stubs::` counterpart — that file exists because SMB
            // browsing is macOS-only, and stubbing SFTP would turn it off on Linux, where
            // the Docker E2E lane runs.
            cfg(any(target_os = "macos", target_os = "linux")) {
                typed: [
                    crate::commands::sftp::connect_sftp_volume,
                    crate::commands::sftp::disconnect_sftp_volume,
                    crate::commands::sftp::approve_sftp_host_key,
                    crate::commands::sftp::forget_sftp_host_key,
                    crate::commands::sftp::list_trusted_sftp_host_keys,
                    crate::commands::sftp::save_sftp_credentials,
                    crate::commands::sftp::has_sftp_credentials,
                    crate::commands::sftp::delete_sftp_credentials,
                    crate::commands::sftp::get_known_sftp_servers,
                    crate::commands::sftp::update_known_sftp_server,
                    crate::commands::sftp::forget_known_sftp_server,
                    crate::commands::sftp::get_sftp_unattended_reconnect,
                    crate::commands::sftp::cancel_sftp_connect,
                ]
                dispatch_only: []
            }
            cfg(not(any(target_os = "macos", target_os = "linux"))) {
                typed: [
                    crate::stubs::network::ensure_network_discovery_started,
                    crate::stubs::network::set_network_enabled,
                    crate::stubs::network::list_network_hosts,
                    crate::stubs::network::resolve_host,
                    crate::stubs::network::connect_to_server,
                    crate::stubs::network::get_network_discovery_state,
                    crate::stubs::network::list_shares_on_host,
                    crate::stubs::network::prefetch_shares,
                    crate::stubs::network::get_host_auth_mode,
                    crate::stubs::network::get_known_shares,
                    crate::stubs::network::get_known_share_by_name,
                    crate::stubs::network::update_known_share,
                    crate::stubs::network::get_username_hints,
                    crate::stubs::network::save_smb_credentials,
                    crate::stubs::network::get_smb_credentials,
                    crate::stubs::network::has_smb_credentials,
                    crate::stubs::network::delete_smb_credentials,
                    crate::stubs::network::is_using_credential_file_fallback,
                    crate::stubs::network::list_shares_with_credentials,
                    crate::stubs::network::mount_network_share,
                    crate::stubs::network::upgrade_to_smb_volume,
                    crate::stubs::network::upgrade_to_smb_volume_with_credentials,
                    crate::stubs::network::system_has_saved_smb_password,
                    crate::stubs::network::upgrade_to_smb_volume_using_saved_password,
                    crate::stubs::network::reconnect_smb_volume,
                    crate::stubs::network::reconnect_smb_volume_with_credentials,
                    crate::stubs::network::get_volume_sign_in_state,
                    crate::stubs::network::disconnect_smb_volume,
                    crate::stubs::network::remove_manual_server,
                    crate::stubs::network::disconnect_network_host,
                ]
                dispatch_only: []
            }
            // Accent color.
            cfg(target_os = "macos") {
                typed: [
                    crate::accent_color::get_accent_color,
                ]
                dispatch_only: []
            }
            cfg(target_os = "linux") {
                typed: [
                    crate::accent_color_linux::get_accent_color,
                ]
                dispatch_only: []
            }
            cfg(not(any(target_os = "macos", target_os = "linux"))) {
                typed: [
                    crate::stubs::accent_color::get_accent_color,
                ]
                dispatch_only: []
            }
            // Reduce transparency.
            cfg(target_os = "macos") {
                typed: [
                    crate::reduce_transparency::get_should_reduce_transparency,
                ]
                dispatch_only: []
            }
            cfg(not(target_os = "macos")) {
                typed: [
                    crate::stubs::reduce_transparency::get_should_reduce_transparency,
                ]
                dispatch_only: []
            }
            // System text size.
            cfg(target_os = "macos") {
                typed: [
                    crate::text_size::get_system_text_size_multiplier,
                ]
                dispatch_only: []
            }
            cfg(not(target_os = "macos")) {
                typed: [
                    crate::stubs::text_size::get_system_text_size_multiplier,
                ]
                dispatch_only: []
            }
            // Permissions.
            cfg(target_os = "macos") {
                typed: [
                    crate::permissions::check_full_disk_access,
                    crate::permissions::check_full_disk_access_quiet,
                    crate::permissions::get_macos_major_version,
                    crate::permissions::open_privacy_settings,
                    crate::permissions::open_appearance_settings,
                    crate::permissions::open_system_settings_url,
                ]
                dispatch_only: []
            }
            cfg(target_os = "linux") {
                typed: [
                    crate::permissions_linux::check_full_disk_access,
                    crate::permissions_linux::check_full_disk_access_quiet,
                    crate::permissions_linux::get_macos_major_version,
                    crate::permissions_linux::open_privacy_settings,
                    crate::permissions_linux::open_appearance_settings,
                    crate::permissions_linux::open_system_settings_url,
                ]
                dispatch_only: []
            }
            cfg(not(any(target_os = "macos", target_os = "linux"))) {
                typed: [
                    crate::stubs::permissions::check_full_disk_access,
                    crate::stubs::permissions::check_full_disk_access_quiet,
                    crate::stubs::permissions::get_macos_major_version,
                    crate::stubs::permissions::open_privacy_settings,
                    crate::stubs::permissions::open_appearance_settings,
                    crate::stubs::permissions::open_system_settings_url,
                ]
                dispatch_only: []
            }
            // "What is Cmdr holding right now?" — the memory diagnostic surface. macOS
            // only (the Mach queries behind it don't exist elsewhere), and deliberately
            // NOT `debug_assertions`-gated: the readings that matter come from a shipped
            // build under a real workload, which is the one condition a debug-only
            // command can't reach.
            cfg(target_os = "macos") {
                typed: [
                    crate::commands::memory_diagnostics::get_memory_diagnostics,
                ]
                dispatch_only: []
            }
            // The custom updater.
            cfg(target_os = "macos") {
                typed: [
                    crate::updater::check_for_update,
                    crate::updater::download_update,
                    crate::updater::install_update,
                ]
                dispatch_only: []
            }
            // E2E-only commands.
            cfg(feature = "playwright-e2e") {
                typed: [
                    crate::commands::file_system::inject_listing_error,
                    crate::commands::file_system::fail_next_brief_column_widths,
                    crate::commands::e2e::set_test_throttle,
                    crate::commands::e2e::set_test_scan_preview_delay,
                    crate::commands::e2e::flush_file_watcher,
                    crate::commands::e2e::force_agent_wake,
                ]
                dispatch_only: [
                    // The specs read these two through raw `__TAURI_INTERNALS__.invoke`
                    // (`test/e2e-playwright/helpers/core.ts`).
                    crate::commands::file_actions::e2e_opened_paths,
                    crate::commands::file_actions::e2e_clear_opened_paths,
                ]
            }
            // The dialog gallery's fixtures, which outlive `debug_assertions`: an E2E build is a
            // RELEASE build, and `dialog-inset.spec.ts` drives the gallery there.
            cfg(any(debug_assertions, feature = "playwright-e2e")) {
                typed: [
                    crate::commands::file_system::create_dialog_gallery_fixtures,
                ]
                dispatch_only: []
            }
            // Debug-build helpers.
            cfg(debug_assertions) {
                typed: [
                    crate::commands::error_reporter::save_error_report_to_disk,
                    crate::commands::file_system::preview_friendly_error,
                ]
                dispatch_only: []
            }
        }
    };
}

/// Expands the manifest into the runtime dispatch table: every command, typed or not, with
/// its group's `#[cfg]` inline, which is the one thing `tauri::generate_handler![]` accepts
/// that `collect_functions!` doesn't.
///
/// Gotcha: paths arrive segment by segment (`$($seg:ident)::+`) rather than as a `path`
/// fragment. A captured `$x:path` reaches a proc macro wrapped in an invisible token group,
/// and both macros below feed proc macros.
macro_rules! build_invoke_handler {
    ($(
        cfg($cfg:meta) {
            typed $(unless cfg($held_back:meta))?: [$($($typed:ident)::+,)*]
            dispatch_only: [$($($dispatch_only:ident)::+,)*]
        }
    )*) => {
        tauri::generate_handler![
            $(
                $(#[cfg($cfg)] $($typed)::+,)*
                $(#[cfg($cfg)] $($dispatch_only)::+,)*
            )*
        ]
    };
}

/// Expands the manifest into `collect_all_types`: one `collect_functions!` block per group,
/// each carrying the group's `#[cfg]` (which is why the groups exist at all), in manifest
/// order, which is the order commands land in `bindings.ts`.
macro_rules! define_type_collectors {
    ($(
        cfg($cfg:meta) {
            typed $(unless cfg($held_back:meta))?: [$($($typed:ident)::+,)*]
            dispatch_only: [$($($dispatch_only:ident)::+,)*]
        }
    )*) => {
        /// Every command signature specta can describe, gathered once per process for
        /// `tauri_specta::internal::command` and so for the exported `bindings.ts`.
        fn collect_all_types(types: &mut specta::Types) -> Vec<specta::datatype::Function> {
            let mut all = vec![];
            $(
                #[cfg($cfg)]
                $(#[cfg(not($held_back))])?
                {
                    use specta::function::collect_functions;
                    all.extend(collect_functions![$($($typed)::+,)*](types));
                }
            )*
            all
        }
    };
}

ipc_command_manifest!(define_type_collectors);

/// Returns the [`tauri_specta::Builder`] holding every command and event the app
/// exposes. Call once from [`crate::run`] and pass
/// `.invoke_handler(builder.invoke_handler())` to `tauri::Builder::default()`.
pub fn builder() -> Builder<tauri::Wry> {
    let runtime_handler: Box<tauri::ipc::InvokeHandler<tauri::Wry>> =
        Box::new(ipc_command_manifest!(build_invoke_handler));

    // Build the final Commands combining the runtime handler with all type info.
    // `internal::command` takes the handler fn and the type-collector fn pointer.
    let combined_commands = tauri_specta::internal::command(runtime_handler, collect_all_types);
    Builder::<tauri::Wry>::new()
        .commands(combined_commands)
        // Typed events. Each registered struct derives `tauri_specta::Event`;
        // its kebab-cased name is the wire event name and its TS type + a typed
        // `events.<name>.listen(...)` helper are generated into `bindings.ts`.
        // Mounted onto the app via `mount_events` in `crate::run`.
        .events(collect_events![
            VolumeSpaceChanged,
            // Write-operations sink (file_system/write_operations/types.rs `TauriEventSink`).
            WriteProgressEvent,
            WriteCompleteEvent,
            WriteCancelledEvent,
            WriteErrorEvent,
            WriteConflictEvent,
            WriteConflictResolvedEvent,
            WriteSourceItemDoneEvent,
            ScanProgressEvent,
            ConflictInfo, // scan-conflict
            DryRunResult, // dry-run-complete
            WriteSettledEvent,
            // Operation manager registry snapshot (write_operations/manager.rs).
            OperationsChanged,
            SuggestionsChanged,
            // Every Ask Cmdr turn's progress, keyed by conversation: rail sends and wakes
            // alike (agent/chat/stream.rs).
            AskCmdrTurn,
            // What the status corner's wake indicator shows: a wake thinking, and which gate is
            // in the way when it can't (agent/wake/indicator.rs).
            AgentWakeStatus,
            // A wake left proposals behind, so the main window can say so once
            // (agent/wake/staged.rs).
            AgentWakeStaged,
            // The quit gate holding an exit while operations run (quit/).
            QuitRequested,
            // Listing sink (file_system/listing/streaming.rs `TauriListingEventSink`).
            ListingOpeningEvent,
            ListingProgressEvent,
            ListingReadCompleteEvent,
            ListingCompleteEvent,
            ListingErrorEvent,
            ListingCancelledEvent,
            // Scan-preview (file_system/write_operations/scan_preview.rs).
            ScanPreviewProgressEvent,
            ScanPreviewCompleteEvent,
            ScanPreviewErrorEvent,
            ScanPreviewCancelledEvent,
            // Volumes + disk space (volumes/, volumes_linux/, space_poller.rs,
            // write_operations/state.rs busy set, menu eject action).
            VolumesChanged,
            VolumeMounted,
            VolumeUnmounted,
            VolumesBusyChanged,
            VolumeContextAction,
            LowDiskSpacePayload, // event_name = "low-disk-space"
            // Session health of a connecting volume. Backend-neutral: SMB emits it
            // today (crates/cmdr-smb/src/volume/), the next connecting backend
            // reuses it. The type lives in `network/mod.rs` so it resolves on every
            // platform (see its doc comment).
            VolumeConnectionChanged,
            // A share Cmdr couldn't take over stays on the macOS kernel mount. Same
            // reason this type lives in `network/mod.rs`: it has to resolve here on
            // every platform.
            SmbFellBackToOsMount,
            // Indexing (indexing/, commands/search.rs). Each pins its wire name
            // via `event_name` because the struct names carry an `…Event` suffix
            // (or live in a differently-named module) that wouldn't kebab-case to
            // the existing wire string.
            IndexScanStartedEvent,           // event_name = "index-scan-started"
            IndexScanProgressEvent,          // event_name = "index-scan-progress"
            IndexScanCompleteEvent,          // event_name = "index-scan-complete"
            IndexScanAbortedEvent,           // event_name = "index-scan-aborted"
            IndexCoverageBranchStartedEvent, // event_name = "index-coverage-branch-started"
            IndexCoveragePhaseStartedEvent,  // event_name = "index-coverage-phase-started"
            IndexCoverageBranchEndedEvent,   // event_name = "index-coverage-branch-ended"
            IndexPhaseChangedEvent,          // event_name = "index-phase-changed"
            IndexDirUpdatedEvent,            // event_name = "index-dir-updated"
            IndexReplayProgressEvent,        // event_name = "index-replay-progress"
            IndexReplayCompleteEvent,        // event_name = "index-replay-complete"
            IndexRescanNotificationEvent,    // event_name = "index-rescan-notification"
            AggregationProgressEvent,        // event_name = "index-aggregation-progress"
            IndexAggregationCompleteEvent,   // event_name = "index-aggregation-complete" (payloadless)
            IndexMemoryWarningEvent,         // event_name = "index-memory-warning"
            IndexFreshnessChangedEvent,      // event_name = "index-freshness-changed"
            SearchIndexReadyEvent,           // event_name = "search-index-ready"
            // Live search (search/live/events.rs `TauriSearchEventSink`): one
            // progress stream plus one terminal event, every one stamped with the
            // run it belongs to.
            SearchProgressEvent,
            SearchCompleteEvent,
            SearchCancelledEvent,
            SearchErrorEvent,
            // Image enrichment progress (media_index/events.rs): image
            // indexing joins the top-right indicator as a second publisher.
            MediaEnrichProgressEvent, // event_name = "media-enrich-progress"
            MediaEnrichTerminalEvent, // event_name = "media-enrich-terminal"
            // MTP device events (mtp/connection/, mtp/watcher.rs). Struct names
            // kebab-case directly to the wire names, so no `event_name` override.
            MtpDeviceConnected,
            MtpDeviceDisconnected,
            MtpStorageRemoved,
            MtpExclusiveAccessError,
            MtpPermissionError,
            MtpPtpcameradSuppressed,
            MtpPtpcameradRestored,
            // Network + git (network/, file_system/git/, menu/menu_handlers.rs).
            // Host-found / host-resolved flatten the bare
            // `NetworkHost`; `git-state-changed` pins its wire name via `event_name`
            // (the `…Payload` suffix wouldn't kebab-case to it); `network-host-context-action`
            // is window-scoped (`emit_to`).
            NetworkHostFound,
            NetworkHostLost,
            NetworkHostResolved,
            NetworkDiscoveryStateChanged,
            NetworkHostContextAction,
            GitStateChangedPayload, // event_name = "git-state-changed"
            // AI + system/misc events.
            // AI lifecycle (ai/manager.rs, ai/download.rs). The payloadless ones
            // are unit structs (`type X = null`); `DownloadProgress` pins its
            // wire name via `event_name` (it kebab-cases to `download-progress`).
            DownloadProgress, // event_name = "ai-download-progress"
            AiStarting,
            AiServerReady,
            AiVerifying,
            AiInstalling,
            AiInstallComplete,
            AiExtracting,
            // Appearance / system (system_events.rs, menu/menu_handlers.rs,
            // commands/ui.rs, downloads/global_shortcut.rs). Scalar emits got
            // wrapped in named structs; the drag structs live in the always-compiled
            // `system_events` because their emit sites are macOS-gated.
            AccentColorChanged,
            ReduceTransparencyChanged,
            SystemTextSizeChanged,
            MenuBarRebuilt,
            OsLocalesChanged,
            SettingsChanged,
            ViewModeChanged,           // emit_to("main")
            MenuSort,                  // emit_to("main")
            MediaIndexFolderExclusion, // emit_to("main") = "media-index-folder-exclusion"
            MediaIndexFolderChoice,    // emit_to("main") = "media-index-folder-choice"
            GlobalShortcutFired,
            DragImageSize,
            DragModifiers,
            QuickLookKeyEvent, // event_name = "quick-look-key"
            QuickLookClosed,   // payloadless
            // Directory watcher (file_system/watcher.rs, listing/diff_emitter.rs).
            DirectoryDiff,
            DirectoryDeletedEvent, // event_name = "directory-deleted"
            // Downloads sink (downloads/watcher.rs `AppHandleSink`).
            DownloadDetectedEvent, // event_name = "download-detected"
            // Const-named events (the wire string used to live in a `const`).
            RestrictedPathsChangedPayload, // event_name = "restricted-paths-changed"
            SessionStartedEvent,           // event_name = "drag-out-session-started"
            SessionCompleteEvent,          // event_name = "drag-out-session-complete"
            ErrorReportAutoSent,
            // Window management: `emit_to`-targeted window lifecycle
            // (mcp/executor/, menu/menu_handlers.rs, commands/settings.rs). Struct
            // names kebab-case directly to the wire names, so no `event_name`
            // overrides. `execute-command` is also FE-emitted (LicenseSection).
            ExecuteCommand,
            OpenSettings,
            OpenFileViewer,
            FocusSettings,
            FocusFileViewer,
            FocusAbout,
            FocusConfirmation,
            CloseFileViewer,
            CloseAllFileViewers,
            CloseAbout,
            CloseConfirmation,
            McpSettingsClose,
            ViewerWordWrapToggled,
            TabContextAction,
            PersistRestrictedSetting,
            // FE-emitted, like `execute-command`: the queue window asks the main
            // window to foreground one operation, and the settings window asks it to show
            // the agent's memory folder in a pane.
            ForegroundOperation,
            RevealPath,
        ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerates `apps/desktop/src/lib/ipc/bindings.ts`.
    ///
    /// Marked `#[ignore]` so it doesn't fire on every `cargo nextest run`:
    /// it has the side effect of writing to disk, which would silently mutate
    /// the working tree on every test run. The canonical entry point is
    /// `pnpm bindings:regen` (from the desktop app dir or repo root via the
    /// dev script), which runs this test and then `oxfmt` on the output so
    /// the result lands in project format.
    ///
    /// CI's `bindings-fresh` check runs the same flow and fails if the
    /// committed `bindings.ts` differs from a fresh regen.
    #[test]
    #[ignore = "side-effect: rewrites bindings.ts; run via `pnpm bindings:regen` or with --run-ignored=ignored-only"]
    fn export_bindings_test() {
        use specta_typescript::Typescript;

        let b = builder();
        let out_path = "../src/lib/ipc/bindings.ts";
        b.export(
            Typescript::default().header("// AUTO-GENERATED: do not edit. Regenerate with `pnpm bindings:regen`.\n"),
            out_path,
        )
        .expect("Failed to export bindings");
    }

    fn exported_command_names() -> Vec<String> {
        let mut types = specta::Types::default();
        collect_all_types(&mut types)
            .iter()
            .map(|f| f.name().to_string())
            .collect()
    }

    /// The exported bindings must not move when a lane turns on `virtual-mtp`, or the whole
    /// point of every cargo lane sharing one feature set collapses: the regen would produce a
    /// file that disagrees with the committed one, and `bindings-fresh` would "fix" it by
    /// committing three commands a real build doesn't answer. This test runs in exactly the
    /// build where the export runs, which is what the manifest's `unless cfg(test)` covers.
    #[test]
    fn the_exported_surface_leaves_out_the_test_only_virtual_mtp_commands() {
        let names = exported_command_names();
        for held_back in [
            "rescan_virtual_mtp",
            "pause_virtual_mtp_watcher",
            "resume_virtual_mtp_watcher",
        ] {
            assert!(
                !names.contains(&held_back.to_string()),
                "`{held_back}` reached the specta collector in a test build, so \
                 `pnpm bindings:regen` would write it into bindings.ts"
            );
        }
    }

    /// Two manifest groups claiming the same command would emit it twice into `bindings.ts`,
    /// which is a broken TypeScript object literal rather than a loud failure.
    #[test]
    fn no_command_reaches_the_exported_surface_twice() {
        let mut names = exported_command_names();
        names.sort();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "a command appears in more than one manifest group");
    }
}
