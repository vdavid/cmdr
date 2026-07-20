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
//! - In debug builds we call [`builder().export(...)`] to regenerate
//!   `apps/desktop/src/lib/ipc/bindings.ts` on each launch (that's the only place the bindings are
//!   written to disk; everything else just imports them).
//!
//! ## File layout
//!
//! - `ipc.rs` (this file) holds the `builder()` entry point, the runtime
//!   `tauri::generate_handler![]` dispatch macro call, and the `greet` smoke test command.
//! - [`crate::ipc_collectors`] holds the per-platform `collect_*_types` helpers (one per `#[cfg]`
//!   group) plus the `collect_all_types` combiner that `builder()` hands to
//!   `tauri_specta::internal::command`.
//!
//! ## Platform-specific commands and `collect_commands!`
//!
//! `collect_commands!` doesn't support `#[cfg(...)]` inline attributes because
//! the macro only accepts path expressions. We work around this by:
//!
//! 1. Using `tauri::generate_handler![]` (which supports `#[cfg(...)`) for the runtime invoke
//!    handler.
//! 2. Using `specta::function::collect_functions![]` in static functions, one per platform group,
//!    to collect type info separately.
//! 3. Combining everything via `tauri_specta::internal::command` which accepts a runtime handler
//!    and a type-collector fn pointer.

#[cfg(test)]
use specta_typescript::Typescript;
use tauri_specta::{Builder, collect_events};

use crate::commands::search::SearchIndexReadyEvent;
use crate::file_system::git::watcher::GitStateChangedPayload;
use crate::file_system::listing::streaming::{
    ListingCancelledEvent, ListingCompleteEvent, ListingErrorEvent, ListingOpeningEvent, ListingProgressEvent,
    ListingReadCompleteEvent,
};
use crate::file_system::write_operations::{
    ConflictInfo, DryRunResult, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteErrorEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use crate::file_system::write_operations::{OperationsChanged, VolumesBusyChanged};
use crate::indexing::writer::AggregationProgressEvent;
use crate::indexing::{
    IndexAggregationCompleteEvent, IndexDirUpdatedEvent, IndexFreshnessChangedEvent, IndexMemoryWarningEvent,
    IndexPhaseChangedEvent, IndexReplayCompleteEvent, IndexReplayProgressEvent, IndexRescanNotificationEvent,
    IndexScanAbortedEvent, IndexScanCompleteEvent, IndexScanProgressEvent, IndexScanStartedEvent,
};
use crate::ipc_collectors::collect_all_types;
use crate::media_index::events::{MediaEnrichProgressEvent, MediaEnrichTerminalEvent};
use crate::mtp::{
    MtpDeviceConnected, MtpDeviceDisconnected, MtpExclusiveAccessError, MtpPermissionError, MtpPtpcameradRestored,
    MtpPtpcameradSuppressed, MtpStorageRemoved,
};
use crate::network::{
    NetworkDiscoveryStateChanged, NetworkHostContextAction, NetworkHostFound, NetworkHostLost, NetworkHostResolved,
    SmbConnectionChanged,
};
use crate::space_poller::{LowDiskSpacePayload, VolumeSpaceChanged};
use crate::volume_broadcast::{VolumeContextAction, VolumeMounted, VolumeUnmounted, VolumesChanged};
// Window-management events: emit_to-targeted window lifecycle.
use crate::window_events::{
    CloseAbout, CloseAllFileViewers, CloseConfirmation, CloseFileViewer, ExecuteCommand, FocusAbout, FocusConfirmation,
    FocusFileViewer, FocusSettings, McpSettingsClose, OpenFileViewer, OpenSettings, PersistRestrictedSetting,
    TabContextAction, ViewerWordWrapToggled,
};
// AI + system/misc events.
use crate::ai::{
    AiExtracting, AiInstallComplete, AiInstalling, AiServerReady, AiStarting, AiVerifying, DownloadProgress,
};
use crate::downloads::global_shortcut::GlobalShortcutFired;
use crate::downloads::watcher::DownloadDetectedEvent;
use crate::error_reporter::auto_dispatcher::ErrorReportAutoSent;
use crate::file_system::watcher::{DirectoryDeletedEvent, DirectoryDiff};
use crate::menu::{MediaIndexFolderExclusion, MenuSort, SettingsChanged, ViewModeChanged};
use crate::quick_look::{QuickLookClosed, QuickLookKeyEvent};
use crate::restricted_paths::RestrictedPathsChangedPayload;
use crate::system_events::{
    AccentColorChanged, DragImageSize, DragModifiers, ReduceTransparencyChanged, SessionCompleteEvent,
    SessionStartedEvent, SystemTextSizeChanged,
};

/// Public greeting used by the example webview surface; kept here as the
/// foundational smoke test for the specta wiring.
#[tauri::command]
#[specta::specta]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Returns the [`tauri_specta::Builder`] holding every command (and event,
/// once those are wired up) the app exposes. Call once from
/// [`crate::run`] and pass `.invoke_handler(builder.invoke_handler())` to
/// `tauri::Builder::default()`.
pub fn builder() -> Builder<tauri::Wry> {
    // Runtime dispatch: uses tauri::generate_handler![] which properly handles
    // #[cfg(...)] for platform-specific command selection.
    let runtime_handler: Box<tauri::ipc::InvokeHandler<tauri::Wry>> = Box::new(tauri::generate_handler![
        greet,
        crate::commands::file_system::list_directory_start,
        crate::commands::file_system::list_directory_start_streaming,
        crate::commands::file_system::cancel_listing,
        crate::commands::file_system::list_directory_end,
        crate::commands::file_system::refresh_listing,
        crate::commands::file_system::get_file_range,
        crate::commands::file_system::get_file_at,
        crate::commands::file_system::get_paths_at_indices,
        crate::commands::file_system::get_files_at_indices,
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
        crate::commands::font_metrics::store_font_metrics,
        crate::commands::font_metrics::has_font_metrics,
        crate::commands::icons::get_icons,
        crate::commands::icons::get_custom_folder_icon_ids,
        crate::commands::icons::refresh_directory_icons,
        crate::commands::icons::clear_extension_icon_cache,
        crate::commands::icons::clear_directory_icon_cache,
        crate::commands::menu::show_file_context_menu,
        crate::commands::menu::show_breadcrumb_context_menu,
        crate::commands::menu::show_volume_row_context_menu,
        crate::commands::menu::show_parent_row_context_menu,
        crate::commands::menu::show_tab_context_menu,
        crate::commands::menu::show_network_host_context_menu,
        crate::commands::menu::update_pin_tab_menu,
        crate::commands::menu::set_reopen_closed_tab_enabled,
        crate::commands::window_ordering::show_main_window,
        crate::commands::window_ordering::order_window_to_back,
        crate::commands::menu::update_menu_context,
        crate::commands::menu::activate_window_menu,
        crate::commands::menu::toggle_hidden_files,
        crate::commands::menu::sync_menu_show_hidden,
        crate::commands::menu::update_view_mode_menu,
        crate::commands::file_actions::show_in_finder,
        crate::commands::file_actions::copy_to_clipboard,
        crate::commands::quick_look::quick_look_open,
        crate::commands::quick_look::quick_look_set_path,
        crate::commands::quick_look::quick_look_close,
        crate::commands::file_actions::get_info,
        crate::commands::file_actions::open_in_editor,
        crate::commands::file_actions::open_path,
        #[cfg(feature = "playwright-e2e")]
        crate::commands::file_actions::e2e_opened_paths,
        #[cfg(feature = "playwright-e2e")]
        crate::commands::file_actions::e2e_clear_opened_paths,
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::set_mtp_enabled,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::list_mtp_devices,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::connect_mtp_device,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::disconnect_mtp_device,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::get_mtp_device_info,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::get_ptpcamerad_workaround_command,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::get_mtp_storages,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::list_mtp_directory,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::delete_mtp_object,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::create_mtp_folder,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::rename_mtp_object,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::move_mtp_object,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::scan_mtp_for_copy,
        #[cfg(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux")))]
        crate::commands::mtp::rescan_virtual_mtp,
        #[cfg(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux")))]
        crate::commands::mtp::pause_virtual_mtp_watcher,
        #[cfg(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux")))]
        crate::commands::mtp::resume_virtual_mtp_watcher,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::set_mtp_enabled,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::list_mtp_devices,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::connect_mtp_device,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::disconnect_mtp_device,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::get_mtp_device_info,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::get_ptpcamerad_workaround_command,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::get_mtp_storages,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::list_mtp_directory,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::delete_mtp_object,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::create_mtp_folder,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::rename_mtp_object,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::move_mtp_object,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::scan_mtp_for_copy,
        crate::volume_broadcast::refresh_volumes,
        crate::space_poller::watch_volume_space,
        crate::space_poller::unwatch_volume_space,
        crate::space_poller::set_disk_space_threshold,
        crate::space_poller::set_low_disk_space_config,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::list_volumes,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::get_default_volume_id,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::get_volume_space,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::resolve_path_volume,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::resolve_location,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::list_volumes,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::get_default_volume_id,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::get_volume_space,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::resolve_path_volume,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::resolve_location,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::list_volumes,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::get_default_volume_id,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::get_volume_space,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::resolve_path_volume,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::resolve_location,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::list_network_hosts,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::get_network_discovery_state,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::resolve_host,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::list_shares_on_host,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::prefetch_shares,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::get_host_auth_mode,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::get_known_shares,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::get_known_share_by_name,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::update_known_share,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::get_username_hints,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::save_smb_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::get_smb_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::has_smb_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::delete_smb_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::is_using_credential_file_fallback,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::list_shares_with_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::mount_network_share,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::upgrade_to_smb_volume,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::upgrade_to_smb_volume_with_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::system_has_saved_smb_password,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::upgrade_to_smb_volume_using_saved_password,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::reconnect_smb_volume,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::reconnect_smb_volume_with_credentials,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::disconnect_smb_volume,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::eject::eject_volume,
        crate::commands::eject::get_busy_volume_ids,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::connect_to_server,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::remove_manual_server,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::disconnect_network_host,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::ensure_network_discovery_started,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::set_network_enabled,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::ensure_network_discovery_started,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::set_network_enabled,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::list_network_hosts,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::get_network_discovery_state,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::resolve_host,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::list_shares_on_host,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::prefetch_shares,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::get_host_auth_mode,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::get_known_shares,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::get_known_share_by_name,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::update_known_share,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::get_username_hints,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::save_smb_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::get_smb_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::has_smb_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::delete_smb_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::is_using_credential_file_fallback,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::list_shares_with_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::mount_network_share,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::upgrade_to_smb_volume,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::upgrade_to_smb_volume_with_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::system_has_saved_smb_password,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::upgrade_to_smb_volume_using_saved_password,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::reconnect_smb_volume,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::reconnect_smb_volume_with_credentials,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::disconnect_smb_volume,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::connect_to_server,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::remove_manual_server,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::network::disconnect_network_host,
        #[cfg(target_os = "macos")]
        crate::accent_color::get_accent_color,
        #[cfg(target_os = "linux")]
        crate::accent_color_linux::get_accent_color,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::accent_color::get_accent_color,
        #[cfg(target_os = "macos")]
        crate::reduce_transparency::get_should_reduce_transparency,
        #[cfg(not(target_os = "macos"))]
        crate::stubs::reduce_transparency::get_should_reduce_transparency,
        #[cfg(target_os = "macos")]
        crate::text_size::get_system_text_size_multiplier,
        #[cfg(not(target_os = "macos"))]
        crate::stubs::text_size::get_system_text_size_multiplier,
        #[cfg(target_os = "macos")]
        crate::permissions::check_full_disk_access,
        #[cfg(target_os = "macos")]
        crate::permissions::check_full_disk_access_quiet,
        #[cfg(target_os = "macos")]
        crate::permissions::get_macos_major_version,
        #[cfg(target_os = "macos")]
        crate::permissions::open_privacy_settings,
        #[cfg(target_os = "macos")]
        crate::permissions::open_appearance_settings,
        #[cfg(target_os = "macos")]
        crate::permissions::open_system_settings_url,
        #[cfg(target_os = "linux")]
        crate::permissions_linux::check_full_disk_access,
        #[cfg(target_os = "linux")]
        crate::permissions_linux::check_full_disk_access_quiet,
        #[cfg(target_os = "linux")]
        crate::permissions_linux::get_macos_major_version,
        #[cfg(target_os = "linux")]
        crate::permissions_linux::open_privacy_settings,
        #[cfg(target_os = "linux")]
        crate::permissions_linux::open_appearance_settings,
        #[cfg(target_os = "linux")]
        crate::permissions_linux::open_system_settings_url,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::check_full_disk_access,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::check_full_disk_access_quiet,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::get_macos_major_version,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::open_privacy_settings,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::open_appearance_settings,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::open_system_settings_url,
        crate::commands::analytics::track_event,
        crate::commands::beta_signup::beta_signup,
        crate::commands::crash_reporter::check_pending_crash_report,
        crate::commands::crash_reporter::dismiss_crash_report,
        crate::commands::crash_reporter::send_crash_report,
        crate::commands::error_reporter::prepare_error_report_preview,
        crate::commands::error_reporter::send_error_report,
        crate::commands::error_reporter::record_breadcrumb,
        crate::commands::error_reporter::record_settings_defaults,
        #[cfg(debug_assertions)]
        crate::commands::error_reporter::save_error_report_to_disk,
        crate::commands::feedback::send_feedback,
        crate::commands::licensing::get_license_status,
        crate::commands::licensing::get_window_title,
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
        crate::ai::manager::configure_ai,
        crate::ai::server::start_ai_server,
        crate::ai::server::stop_ai_server,
        crate::ai::connection_check::check_ai_connection,
        crate::system_memory::get_system_memory_info,
        crate::system_strings::get_localized_system_strings,
        crate::ai::install::start_ai_download,
        crate::ai::install::cancel_ai_download,
        crate::ai::install::uninstall_ai,
        crate::ai::api_keys::save_ai_api_key,
        crate::ai::api_keys::get_ai_api_key,
        crate::ai::api_keys::delete_ai_api_key,
        crate::ai::api_keys::has_ai_api_key,
        crate::ai::suggestions::get_folder_suggestions,
        // stream_folder_suggestions / cancel_folder_suggestions: streaming via tauri Channel<T>;
        // not specta-friendly yet, kept on raw invoke (eslint opt-out at FE call sites).
        crate::ai::suggestions::stream_folder_suggestions,
        crate::ai::suggestions::cancel_folder_suggestions,
        crate::commands::mcp::set_mcp_enabled,
        crate::commands::mcp::set_mcp_port,
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
        crate::commands::settings::set_filter_safe_save_artifacts_cmd,
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
        crate::commands::indexing::set_indexing_enabled,
        crate::commands::indexing::start_indexing_after_fda_decision,
        crate::commands::indexing::get_index_debug_status,
        crate::commands::indexing::get_volume_index_status,
        crate::commands::indexing::get_volume_index_status_by_id,
        crate::commands::indexing::enable_drive_index,
        crate::commands::indexing::disable_drive_index,
        crate::commands::indexing::forget_drive_index,
        crate::commands::indexing::rescan_drive_index,
        crate::importance::commands::record_visit,
        crate::media_index::commands::media_index_search_ocr,
        crate::media_index::commands::media_index_volume_state,
        crate::media_index::commands::media_index_thumbnail_token,
        crate::media_index::commands::media_index_drop_thumbnail_tokens,
        crate::media_index::commands::media_index_set_network_volume_enabled,
        crate::media_index::commands::media_index_set_always_index_volume,
        crate::media_index::commands::media_index_set_always_index_folder,
        crate::media_index::commands::media_index_set_excluded_folder,
        crate::media_index::commands::media_index_set_importance_threshold,
        crate::media_index::commands::media_index_covered_count,
        crate::media_index::commands::media_index_reclaim_preview,
        crate::media_index::commands::media_index_prune_below_threshold,
        crate::media_index::commands::media_index_find_similar,
        crate::media_index::commands::media_index_dedup_clusters,
        crate::media_index::commands::media_index_search_tag,
        crate::media_index::commands::media_index_search_semantic,
        crate::media_index::commands::media_index_clip_model_status,
        crate::media_index::commands::media_index_download_clip_model,
        crate::commands::search::prepare_search_index,
        crate::commands::search::search_files,
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
        crate::commands::operation_log::get_recent_operation_log_entries,
        crate::commands::operation_log::get_operation_log_detail,
        // ask_cmdr_send_message: streaming via tauri Channel<T>; not specta-friendly, so
        // it rides raw invoke on the frontend and is absent from ipc_collectors.
        crate::commands::agent::ask_cmdr_send_message,
        crate::commands::agent::ask_cmdr_cancel,
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
        #[cfg(feature = "playwright-e2e")]
        crate::commands::e2e::set_test_throttle,
        #[cfg(feature = "playwright-e2e")]
        crate::commands::e2e::flush_file_watcher,
        #[cfg(feature = "playwright-e2e")]
        crate::commands::file_system::inject_listing_error,
        #[cfg(debug_assertions)]
        crate::commands::file_system::preview_friendly_error,
        crate::commands::clipboard::copy_files_to_clipboard,
        crate::commands::clipboard::cut_files_to_clipboard,
        crate::commands::clipboard::copy_paths_to_clipboard,
        crate::commands::clipboard::cut_paths_to_clipboard,
        crate::commands::clipboard::read_clipboard_files,
        crate::commands::clipboard::read_clipboard_text,
        crate::commands::clipboard::paste_clipboard_as_file,
        crate::commands::clipboard::clear_clipboard_cut_state,
        #[cfg(target_os = "macos")]
        crate::updater::check_for_update,
        #[cfg(target_os = "macos")]
        crate::updater::download_update,
        #[cfg(target_os = "macos")]
        crate::updater::install_update,
    ]);

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
            WriteSourceItemDoneEvent,
            ScanProgressEvent,
            ConflictInfo, // scan-conflict
            DryRunResult, // dry-run-complete
            WriteSettledEvent,
            // Operation manager registry snapshot (write_operations/manager.rs).
            OperationsChanged,
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
            // Indexing (indexing/, commands/search.rs). Each pins its wire name
            // via `event_name` because the struct names carry an `…Event` suffix
            // (or live in a differently-named module) that wouldn't kebab-case to
            // the existing wire string.
            IndexScanStartedEvent,         // event_name = "index-scan-started"
            IndexScanProgressEvent,        // event_name = "index-scan-progress"
            IndexScanCompleteEvent,        // event_name = "index-scan-complete"
            IndexScanAbortedEvent,         // event_name = "index-scan-aborted"
            IndexPhaseChangedEvent,        // event_name = "index-phase-changed"
            IndexDirUpdatedEvent,          // event_name = "index-dir-updated"
            IndexReplayProgressEvent,      // event_name = "index-replay-progress"
            IndexReplayCompleteEvent,      // event_name = "index-replay-complete"
            IndexRescanNotificationEvent,  // event_name = "index-rescan-notification"
            AggregationProgressEvent,      // event_name = "index-aggregation-progress"
            IndexAggregationCompleteEvent, // event_name = "index-aggregation-complete" (payloadless)
            IndexMemoryWarningEvent,       // event_name = "index-memory-warning"
            IndexFreshnessChangedEvent,    // event_name = "index-freshness-changed"
            SearchIndexReadyEvent,         // event_name = "search-index-ready"
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
            // Network + git (network/, file_system/git/, file_system/volume/backends/smb/,
            // menu/menu_handlers.rs). Host-found / host-resolved flatten the bare
            // `NetworkHost`; `git-state-changed` pins its wire name via `event_name`
            // (the `…Payload` suffix wouldn't kebab-case to it); `network-host-context-action`
            // is window-scoped (`emit_to`).
            NetworkHostFound,
            NetworkHostLost,
            NetworkHostResolved,
            NetworkDiscoveryStateChanged,
            NetworkHostContextAction,
            SmbConnectionChanged,
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
            SettingsChanged,
            ViewModeChanged,           // emit_to("main")
            MenuSort,                  // emit_to("main")
            MediaIndexFolderExclusion, // emit_to("main") = "media-index-folder-exclusion"
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
        let b = builder();
        let out_path = "../src/lib/ipc/bindings.ts";
        b.export(
            Typescript::default().header("// AUTO-GENERATED: do not edit. Regenerate with `pnpm bindings:regen`.\n"),
            out_path,
        )
        .expect("Failed to export bindings");
    }
}
