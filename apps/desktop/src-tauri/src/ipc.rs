//! Type-safe IPC: typed Rust↔TS bindings for tauri commands and events via tauri-specta.
//!
//! See [`apps/desktop/src/lib/ipc/CLAUDE.md`](../../../src/lib/ipc/CLAUDE.md) for the
//! frontend side and the migration recipe. The convention is documented in
//! `AGENTS.md` § "Type-safe IPC".
//!
//! ## Why
//!
//! Tauri command names used to live as magic strings on both sides — a Rust
//! `#[tauri::command]` plus an `invoke('command_name', args)` on the frontend,
//! with no compile-time link. Renaming the Rust side silently broke runtime
//! IPC with a generic "not allowed" error. Now the frontend imports typed
//! `commands.commandName(args)` from generated bindings, so command-name and
//! argument-shape mismatches surface at `pnpm check`.
//!
//! ## How
//!
//! - Each command has `#[tauri::command]` + `#[specta::specta]`.
//! - Each DTO crossing the IPC boundary has `#[derive(specta::Type)]`.
//! - [`builder()`] returns a [`tauri_specta::Builder`] holding every command
//!   and event the app exposes; [`run`](crate::run) attaches it to
//!   `tauri::Builder::default()` via `.invoke_handler(builder.invoke_handler())`
//!   and `builder.mount_events(app)` in setup.
//! - In debug builds we call [`builder().export(...)`] to regenerate
//!   `apps/desktop/src/lib/ipc/bindings.ts` on each launch — that's the only
//!   place the bindings are written to disk; everything else just imports them.
//!
//! ## Platform-specific commands and `collect_commands!`
//!
//! `collect_commands!` doesn't support `#[cfg(...)]` inline attributes because
//! the macro only accepts path expressions. We work around this by:
//!
//! 1. Using `tauri::generate_handler![]` (which supports `#[cfg(...)`) for the
//!    runtime invoke handler.
//! 2. Using `specta::function::collect_functions![]` in static functions, one
//!    per platform group, to collect type info separately.
//! 3. Combining everything via `tauri_specta::internal::command` which accepts
//!    a runtime handler and a type-collector fn pointer.

use specta::Types;
use specta::datatype::Function;
#[cfg(test)]
use specta_typescript::Typescript;
use tauri_specta::Builder;

/// Public greeting used by the example webview surface; kept here as the
/// foundational smoke test for the specta wiring.
#[tauri::command]
#[specta::specta]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// ─── Static type-collection functions ────────────────────────────────────────
// One function per platform group. Each collects specta type info from
// `collect_functions![]` into a `Types` registry and returns the function list.
// These are plain `fn` items (not closures) so they match the required
// `fn(&mut Types) -> Vec<Function>` signature for `internal::command`.

fn collect_cross_platform_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        greet,
        crate::commands::file_system::list_directory_start,
        crate::commands::file_system::list_directory_start_streaming,
        crate::commands::file_system::cancel_listing,
        crate::commands::file_system::list_directory_end,
        crate::commands::file_system::refresh_listing,
        crate::commands::file_system::get_file_range,
        crate::commands::file_system::get_file_at,
        crate::commands::file_system::get_files_at_indices,
        crate::commands::file_system::get_paths_at_indices,
        crate::commands::file_system::get_total_count,
        crate::commands::file_system::get_brief_column_text_widths,
        crate::commands::file_system::find_file_index,
        crate::commands::file_system::find_file_indices,
        crate::commands::file_system::find_first_fuzzy_match,
        crate::commands::file_system::resort_listing,
        crate::commands::file_system::get_path_limits,
        crate::commands::file_system::path_exists,
        crate::commands::file_system::create_directory,
        crate::commands::file_system::create_file,
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
        crate::commands::file_system::copy_between_volumes,
        crate::commands::file_system::move_between_volumes,
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
        crate::commands::file_viewer::viewer_open,
        crate::commands::file_viewer::viewer_get_lines,
        crate::commands::file_viewer::viewer_get_status,
        crate::commands::file_viewer::viewer_search_start,
        crate::commands::file_viewer::viewer_search_poll,
        crate::commands::file_viewer::viewer_search_cancel,
        crate::commands::file_viewer::viewer_close,
        crate::commands::file_viewer::viewer_setup_menu,
        crate::commands::file_viewer::viewer_set_word_wrap,
        // store_font_metrics is generic (<R: tauri::Runtime>) — excluded from specta collection
        crate::commands::font_metrics::has_font_metrics,
        crate::commands::icons::get_icons,
        crate::commands::icons::refresh_directory_icons,
        crate::commands::icons::clear_extension_icon_cache,
        crate::commands::icons::clear_directory_icon_cache,
        // show_file_context_menu, show_breadcrumb_context_menu, update_pin_tab_menu,
        // show_main_window, update_menu_context, set_menu_context, toggle_hidden_files,
        // update_view_mode_menu, copy_to_clipboard are generic (<R: Runtime>) — excluded
        crate::commands::ui::show_tab_context_menu,
        crate::commands::ui::show_network_host_context_menu,
        crate::commands::ui::show_in_finder,
        crate::commands::ui::quick_look,
        crate::commands::ui::get_info,
        crate::commands::ui::open_in_editor,
        crate::commands::ui::cloud_make_available_offline,
        crate::commands::ui::cloud_remove_download,
        crate::mcp::pane_state::update_left_pane_state,
        crate::mcp::pane_state::update_right_pane_state,
        crate::mcp::pane_state::update_focused_pane,
        crate::mcp::pane_state::update_pane_tabs,
        crate::mcp::dialog_state::notify_dialog_opened,
        crate::mcp::dialog_state::notify_dialog_closed,
        crate::mcp::dialog_state::register_known_dialogs,
        crate::commands::sync_status::get_sync_status,
        crate::volume_broadcast::refresh_volumes,
        crate::space_poller::watch_volume_space,
        crate::space_poller::unwatch_volume_space,
        crate::space_poller::set_disk_space_threshold,
        crate::commands::crash_reporter::check_pending_crash_report,
        crate::commands::crash_reporter::dismiss_crash_report,
        crate::commands::crash_reporter::send_crash_report,
        crate::commands::error_reporter::send_error_report,
        // prepare_error_report_preview: BundleManifest contains Breadcrumb.ctx: Option<Value>
        // which specta can't represent. Excluded; stays in generate_handler![].
        // record_breadcrumb takes Option<serde_json::Value>: excluded; stays in generate_handler![].
        crate::commands::error_reporter::record_settings_defaults,
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
        crate::ai::manager::get_ai_model_info,
        crate::ai::manager::get_ai_runtime_status,
        // configure_ai, start_ai_server, start_ai_download are generic (<R: Runtime>) — excluded
        crate::ai::manager::stop_ai_server,
        crate::ai::manager::check_ai_connection,
        crate::system_memory::get_system_memory_info,
        crate::ai::manager::cancel_ai_download,
        crate::ai::manager::dismiss_ai_offer,
        crate::ai::manager::uninstall_ai,
        crate::ai::manager::opt_out_ai,
        crate::ai::manager::opt_in_ai,
        crate::ai::manager::is_ai_opted_out,
        crate::ai::suggestions::get_folder_suggestions,
        // set_mcp_enabled, set_mcp_port are generic (<R: Runtime>) — excluded from specta
        crate::commands::mcp::get_mcp_running,
        crate::commands::mcp::get_mcp_port,
        crate::commands::settings::check_port_available,
        crate::commands::settings::find_available_port,
        crate::commands::settings::update_file_watcher_debounce,
        crate::commands::settings::update_service_resolve_timeout,
        crate::commands::settings::update_menu_accelerator,
        crate::commands::settings::set_direct_smb_connection,
        crate::commands::settings::set_filter_safe_save_artifacts_cmd,
        crate::commands::settings::set_smb_concurrency_cmd,
        crate::commands::settings::set_max_log_storage_mb,
        crate::commands::settings::set_error_reports_enabled,
        crate::commands::settings::set_show_virtual_git_portal,
        crate::commands::logging::batch_fe_logs,
        crate::commands::logging::set_log_level,
        crate::commands::indexing::start_drive_index,
        crate::commands::indexing::stop_drive_index,
        crate::commands::indexing::get_index_status,
        crate::commands::indexing::get_dir_stats,
        crate::commands::indexing::get_dir_stats_batch,
        crate::commands::indexing::clear_drive_index,
        crate::commands::indexing::set_indexing_enabled,
        crate::commands::indexing::start_indexing_after_fda_decision,
        crate::commands::indexing::get_index_debug_status,
        crate::commands::search::prepare_search_index,
        crate::commands::search::search_files,
        crate::commands::search::release_search_index,
        crate::commands::search::translate_search_query,
        crate::commands::search::parse_search_scope,
        crate::commands::search::get_system_dir_excludes,
        crate::commands::e2e::get_e2e_start_path,
        crate::commands::clipboard::copy_files_to_clipboard,
        crate::commands::clipboard::cut_files_to_clipboard,
        crate::commands::clipboard::read_clipboard_files,
        crate::commands::clipboard::read_clipboard_text,
        crate::commands::clipboard::clear_clipboard_cut_state,
    ](types)
}

#[cfg(debug_assertions)]
fn collect_debug_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::commands::error_reporter::save_error_report_to_disk,
        crate::commands::file_system::preview_friendly_error,
    ](types)
}

// MTP commands (macOS + Linux)
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn collect_mtp_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::commands::mtp::set_mtp_enabled,
        crate::commands::mtp::list_mtp_devices,
        crate::commands::mtp::connect_mtp_device,
        crate::commands::mtp::get_mtp_device_info,
        crate::commands::mtp::disconnect_mtp_device,
        crate::commands::mtp::get_mtp_storages,
        crate::commands::mtp::list_mtp_directory,
        crate::commands::mtp::get_ptpcamerad_workaround_command,
        crate::commands::mtp::download_mtp_file,
        crate::commands::mtp::upload_to_mtp,
        crate::commands::mtp::delete_mtp_object,
        crate::commands::mtp::create_mtp_folder,
        crate::commands::mtp::rename_mtp_object,
        crate::commands::mtp::move_mtp_object,
        crate::commands::mtp::scan_mtp_for_copy,
    ](types)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collect_mtp_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::stubs::mtp::set_mtp_enabled,
        crate::stubs::mtp::list_mtp_devices,
        crate::stubs::mtp::connect_mtp_device,
        crate::stubs::mtp::get_mtp_device_info,
        crate::stubs::mtp::disconnect_mtp_device,
        crate::stubs::mtp::get_mtp_storages,
        crate::stubs::mtp::list_mtp_directory,
        crate::stubs::mtp::get_ptpcamerad_workaround_command,
        crate::stubs::mtp::download_mtp_file,
        crate::stubs::mtp::upload_to_mtp,
        crate::stubs::mtp::delete_mtp_object,
        crate::stubs::mtp::create_mtp_folder,
        crate::stubs::mtp::rename_mtp_object,
        crate::stubs::mtp::move_mtp_object,
        crate::stubs::mtp::scan_mtp_for_copy,
    ](types)
}

// Virtual MTP commands (feature-gated)
#[cfg(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux")))]
fn collect_virtual_mtp_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::commands::mtp::rescan_virtual_mtp,
        crate::commands::mtp::pause_virtual_mtp_watcher,
        crate::commands::mtp::resume_virtual_mtp_watcher,
        crate::commands::mtp::resync_virtual_mtp_after_disk_change,
    ](types)
}
#[cfg(not(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux"))))]
fn collect_virtual_mtp_types(_types: &mut Types) -> Vec<Function> {
    vec![]
}

// Volume commands (platform-specific)
#[cfg(target_os = "macos")]
fn collect_volume_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::commands::volumes::list_volumes,
        crate::commands::volumes::resolve_path_volume,
        crate::commands::volumes::get_default_volume_id,
        crate::commands::volumes::get_volume_space,
    ](types)
}
#[cfg(target_os = "linux")]
fn collect_volume_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::commands::volumes_linux::list_volumes,
        crate::commands::volumes_linux::resolve_path_volume,
        crate::commands::volumes_linux::get_default_volume_id,
        crate::commands::volumes_linux::get_volume_space,
    ](types)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collect_volume_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::stubs::volumes::list_volumes,
        crate::stubs::volumes::resolve_path_volume,
        crate::stubs::volumes::get_default_volume_id,
        crate::stubs::volumes::get_volume_space,
    ](types)
}

// Network commands (macOS + Linux, stubs for other platforms)
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn collect_network_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
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
        crate::commands::network::reconnect_smb_volume,
        crate::commands::network::disconnect_smb_volume,
        crate::commands::network::remove_manual_server,
        crate::commands::network::disconnect_network_host,
        crate::commands::network::ensure_network_discovery_started,
        crate::commands::network::set_network_enabled,
    ](types)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collect_network_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
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
        crate::stubs::network::reconnect_smb_volume,
        crate::stubs::network::disconnect_smb_volume,
        crate::stubs::network::remove_manual_server,
        crate::stubs::network::disconnect_network_host,
    ](types)
}

// Accent color command (platform-specific)
#[cfg(target_os = "macos")]
fn collect_accent_color_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![crate::accent_color::get_accent_color](types)
}
#[cfg(target_os = "linux")]
fn collect_accent_color_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![crate::accent_color_linux::get_accent_color](types)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collect_accent_color_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![crate::stubs::accent_color::get_accent_color](types)
}

// System text size multiplier
#[cfg(target_os = "macos")]
fn collect_text_size_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![crate::text_size::get_system_text_size_multiplier](types)
}
#[cfg(not(target_os = "macos"))]
fn collect_text_size_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![crate::stubs::text_size::get_system_text_size_multiplier](types)
}

// Permission commands (platform-specific)
#[cfg(target_os = "macos")]
fn collect_permission_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::permissions::check_full_disk_access,
        crate::permissions::get_macos_major_version,
        crate::permissions::open_privacy_settings,
        crate::permissions::open_appearance_settings,
        crate::permissions::open_system_settings_url,
    ](types)
}
#[cfg(target_os = "linux")]
fn collect_permission_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::permissions_linux::check_full_disk_access,
        crate::permissions_linux::get_macos_major_version,
        crate::permissions_linux::open_privacy_settings,
        crate::permissions_linux::open_appearance_settings,
        crate::permissions_linux::open_system_settings_url,
    ](types)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collect_permission_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::stubs::permissions::check_full_disk_access,
        crate::stubs::permissions::get_macos_major_version,
        crate::stubs::permissions::open_privacy_settings,
        crate::stubs::permissions::open_appearance_settings,
        crate::stubs::permissions::open_system_settings_url,
    ](types)
}

// Custom updater commands (macOS only)
#[cfg(target_os = "macos")]
fn collect_updater_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::updater::check_for_update,
        crate::updater::download_update,
        crate::updater::install_update,
    ](types)
}
#[cfg(not(target_os = "macos"))]
fn collect_updater_types(_types: &mut Types) -> Vec<Function> {
    vec![]
}

// E2E test commands (feature-gated)
#[cfg(feature = "playwright-e2e")]
fn collect_e2e_types(types: &mut Types) -> Vec<Function> {
    use specta::function::collect_functions;
    collect_functions![
        crate::commands::file_system::inject_listing_error,
        crate::commands::e2e::set_test_throttle,
        crate::commands::e2e::flush_file_watcher,
    ](types)
}
#[cfg(not(feature = "playwright-e2e"))]
fn collect_e2e_types(_types: &mut Types) -> Vec<Function> {
    vec![]
}

/// Combined specta type collector that gathers all command signatures from
/// all platform groups. Called once per process from [`builder()`].
fn collect_all_types(types: &mut Types) -> Vec<Function> {
    let mut all = vec![];
    all.extend(collect_cross_platform_types(types));
    all.extend(collect_mtp_types(types));
    all.extend(collect_virtual_mtp_types(types));
    all.extend(collect_volume_types(types));
    all.extend(collect_network_types(types));
    all.extend(collect_accent_color_types(types));
    all.extend(collect_text_size_types(types));
    all.extend(collect_permission_types(types));
    all.extend(collect_updater_types(types));
    all.extend(collect_e2e_types(types));
    #[cfg(debug_assertions)]
    all.extend(collect_debug_types(types));
    all
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
        crate::commands::file_system::path_exists,
        crate::commands::file_system::create_directory,
        crate::commands::file_system::create_file,
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
        crate::commands::file_system::copy_between_volumes,
        crate::commands::file_system::move_between_volumes,
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
        crate::commands::file_viewer::viewer_open,
        crate::commands::file_viewer::viewer_get_lines,
        crate::commands::file_viewer::viewer_get_status,
        crate::commands::file_viewer::viewer_search_start,
        crate::commands::file_viewer::viewer_search_poll,
        crate::commands::file_viewer::viewer_search_cancel,
        crate::commands::file_viewer::viewer_close,
        crate::commands::file_viewer::viewer_setup_menu,
        crate::commands::file_viewer::viewer_set_word_wrap,
        crate::commands::font_metrics::store_font_metrics,
        crate::commands::font_metrics::has_font_metrics,
        crate::commands::icons::get_icons,
        crate::commands::icons::refresh_directory_icons,
        crate::commands::icons::clear_extension_icon_cache,
        crate::commands::icons::clear_directory_icon_cache,
        crate::commands::ui::show_file_context_menu,
        crate::commands::ui::show_breadcrumb_context_menu,
        crate::commands::ui::show_tab_context_menu,
        crate::commands::ui::show_network_host_context_menu,
        crate::commands::ui::update_pin_tab_menu,
        crate::commands::ui::show_main_window,
        crate::commands::ui::update_menu_context,
        crate::commands::ui::set_menu_context,
        crate::commands::ui::toggle_hidden_files,
        crate::commands::ui::update_view_mode_menu,
        crate::commands::ui::show_in_finder,
        crate::commands::ui::copy_to_clipboard,
        crate::commands::ui::quick_look,
        crate::commands::ui::get_info,
        crate::commands::ui::open_in_editor,
        crate::commands::ui::cloud_make_available_offline,
        crate::commands::ui::cloud_remove_download,
        crate::mcp::pane_state::update_left_pane_state,
        crate::mcp::pane_state::update_right_pane_state,
        crate::mcp::pane_state::update_focused_pane,
        crate::mcp::pane_state::update_pane_tabs,
        crate::mcp::dialog_state::notify_dialog_opened,
        crate::mcp::dialog_state::notify_dialog_closed,
        crate::mcp::dialog_state::register_known_dialogs,
        crate::commands::sync_status::get_sync_status,
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
        crate::commands::mtp::download_mtp_file,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::mtp::upload_to_mtp,
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
        #[cfg(all(feature = "virtual-mtp", any(target_os = "macos", target_os = "linux")))]
        crate::commands::mtp::resync_virtual_mtp_after_disk_change,
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
        crate::stubs::mtp::download_mtp_file,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::mtp::upload_to_mtp,
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
        #[cfg(target_os = "macos")]
        crate::commands::volumes::list_volumes,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::get_default_volume_id,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::get_volume_space,
        #[cfg(target_os = "macos")]
        crate::commands::volumes::resolve_path_volume,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::list_volumes,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::get_default_volume_id,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::get_volume_space,
        #[cfg(target_os = "linux")]
        crate::commands::volumes_linux::resolve_path_volume,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::list_volumes,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::get_default_volume_id,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::get_volume_space,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::volumes::resolve_path_volume,
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
        crate::commands::network::reconnect_smb_volume,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::commands::network::disconnect_smb_volume,
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
        crate::stubs::network::reconnect_smb_volume,
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
        crate::text_size::get_system_text_size_multiplier,
        #[cfg(not(target_os = "macos"))]
        crate::stubs::text_size::get_system_text_size_multiplier,
        #[cfg(target_os = "macos")]
        crate::permissions::check_full_disk_access,
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
        crate::stubs::permissions::get_macos_major_version,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::open_privacy_settings,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::open_appearance_settings,
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        crate::stubs::permissions::open_system_settings_url,
        crate::commands::crash_reporter::check_pending_crash_report,
        crate::commands::crash_reporter::dismiss_crash_report,
        crate::commands::crash_reporter::send_crash_report,
        crate::commands::error_reporter::prepare_error_report_preview,
        crate::commands::error_reporter::send_error_report,
        crate::commands::error_reporter::record_breadcrumb,
        crate::commands::error_reporter::record_settings_defaults,
        #[cfg(debug_assertions)]
        crate::commands::error_reporter::save_error_report_to_disk,
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
        crate::ai::manager::get_ai_model_info,
        crate::ai::manager::get_ai_runtime_status,
        crate::ai::manager::configure_ai,
        crate::ai::manager::start_ai_server,
        crate::ai::manager::stop_ai_server,
        crate::ai::manager::check_ai_connection,
        crate::system_memory::get_system_memory_info,
        crate::ai::manager::start_ai_download,
        crate::ai::manager::cancel_ai_download,
        crate::ai::manager::dismiss_ai_offer,
        crate::ai::manager::uninstall_ai,
        crate::ai::manager::opt_out_ai,
        crate::ai::manager::opt_in_ai,
        crate::ai::manager::is_ai_opted_out,
        crate::ai::suggestions::get_folder_suggestions,
        // stream_folder_suggestions / cancel_folder_suggestions: streaming via tauri Channel<T>;
        // not specta-friendly yet, kept on raw invoke (eslint opt-out at FE call sites).
        crate::ai::suggestions::stream_folder_suggestions,
        crate::ai::suggestions::cancel_folder_suggestions,
        crate::commands::mcp::set_mcp_enabled,
        crate::commands::mcp::set_mcp_port,
        crate::commands::mcp::get_mcp_running,
        crate::commands::mcp::get_mcp_port,
        crate::commands::settings::check_port_available,
        crate::commands::settings::find_available_port,
        crate::commands::settings::update_file_watcher_debounce,
        crate::commands::settings::update_service_resolve_timeout,
        crate::commands::settings::update_menu_accelerator,
        crate::commands::settings::set_direct_smb_connection,
        crate::commands::settings::set_filter_safe_save_artifacts_cmd,
        crate::commands::settings::set_smb_concurrency_cmd,
        crate::commands::settings::set_max_log_storage_mb,
        crate::commands::settings::set_error_reports_enabled,
        crate::commands::settings::set_show_virtual_git_portal,
        crate::commands::logging::batch_fe_logs,
        crate::commands::logging::set_log_level,
        crate::commands::indexing::start_drive_index,
        crate::commands::indexing::stop_drive_index,
        crate::commands::indexing::get_index_status,
        crate::commands::indexing::get_dir_stats,
        crate::commands::indexing::get_dir_stats_batch,
        crate::commands::indexing::clear_drive_index,
        crate::commands::indexing::set_indexing_enabled,
        crate::commands::indexing::start_indexing_after_fda_decision,
        crate::commands::indexing::get_index_debug_status,
        crate::commands::search::prepare_search_index,
        crate::commands::search::search_files,
        crate::commands::search::release_search_index,
        crate::commands::search::translate_search_query,
        crate::commands::search::parse_search_scope,
        crate::commands::search::get_system_dir_excludes,
        crate::commands::e2e::get_e2e_start_path,
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
        crate::commands::clipboard::read_clipboard_files,
        crate::commands::clipboard::read_clipboard_text,
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
    Builder::<tauri::Wry>::new().commands(combined_commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerates `apps/desktop/src/lib/ipc/bindings.ts`.
    ///
    /// Marked `#[ignore]` so it doesn't fire on every `cargo nextest run` —
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
        b.export(
            Typescript::default().header("// AUTO-GENERATED — do not edit. Regenerate with `pnpm bindings:regen`.\n"),
            "../src/lib/ipc/bindings.ts",
        )
        .expect("Failed to export bindings");
    }
}
