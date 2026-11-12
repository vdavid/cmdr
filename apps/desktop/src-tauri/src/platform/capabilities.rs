//! What this OS lets Cmdr offer, as one typed answer the frontend reads instead of asking which OS
//! it runs on. Each arm (`macos.rs`, `linux/mod.rs`, `windows.rs`) answers every field explicitly,
//! and `frontend.rs` hands the answer to every webview. See
//! `apps/desktop/src/lib/platform/DETAILS.md` § Capabilities.

/// One flag per OS-provided feature the UI shows or hides. `true` means the backend can do it on
/// this OS, so the surface for it exists; `false` means it hides on purpose, never by accident.
///
/// Named after the UI that asks. A new field is a compile error in every arm until each OS
/// answers it, and in the browser demo's table (`lib/demo/platform.ts`) through the generated
/// TS type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    /// Quick Look previews of the cursor file (`file.quickLook`).
    pub quick_look: bool,
    /// The OS's own Get Info panel for a file (`file.getInfo`).
    pub get_info: bool,
    /// "Open terminal here": the known-terminals table, the F9 slot, and the Terminal settings card.
    pub open_terminal_here: bool,
    /// Picking the app F4 opens files in (the Text editor settings card and its first-use hint).
    pub text_editor_choice: bool,
    /// Cloud-file actions on synced folders: keep offline, remove download, and the Google Drive
    /// items.
    pub cloud_file_actions: bool,
    /// The system's color tags on files (`tags.toggle*`).
    pub color_tags: bool,
    /// The OS gives the app an app menu that owns Quit, Hide, Hide others, and Show all, keys
    /// included (`app.hide`, `app.hideOthers`, `app.showAll` exist only here; `app.quit` is the
    /// OS's item here and Cmdr's own command elsewhere).
    pub app_menu: bool,
    /// The app menu's Services submenu, fed with the focused pane's selection.
    pub services_menu: bool,
    /// A Full Disk Access permission the user can grant: the onboarding step, the status badge,
    /// and every "open System Settings" offer.
    pub full_disk_access: bool,
    /// Cmdr's Finder extension (the settings card and its nudge).
    pub finder_extension: bool,
    /// Cmdr can answer other apps' "Show in Finder" (the settings card, its nudge and notice).
    pub reveal_handler: bool,
    /// The offer to keep Cmdr in the Dock.
    pub dock_pin: bool,
    /// Installing the `cmdr` terminal command (`app.toggleCliCommand`).
    pub cli_command: bool,
    /// The window's title bar is drawn by the page, under the OS's window buttons, so the page
    /// leaves room for them (`--titlebar-height`). Where `false`, the OS draws the title bar
    /// outside the webview.
    pub overlay_title_bar: bool,
    /// An image engine for OCR and tags, so the Image indexing settings exist.
    pub image_indexing: bool,
    /// A Local Network permission the OS asks the user for (the explainer in Network settings).
    pub local_network_permission: bool,
    /// This build ships a local model runtime, so the Local AI option and its settings card exist.
    /// The backend's `is_local_ai_supported` narrows it further on the running machine.
    pub local_ai: bool,
    /// The OS's document framework writes the `.sb-` safe-save files the listing filter knows (the
    /// "Show safe-save files" setting).
    pub safe_save_files: bool,
    /// A folder named `X.app` is an app the OS opens (the archives settings' bundle row).
    pub app_bundles: bool,
    /// Cmdr offers a way into full screen, so the "Esc exits full screen" setting and its hint
    /// exist.
    pub full_screen: bool,
    /// Cmdr ships as a Homebrew cask (the onboarding note asking for a star on the cask).
    pub homebrew_cask: bool,
    /// A system-wide shortcut works from any app (the global "go to latest download" shortcut).
    pub global_shortcuts: bool,
    /// Cmdr can read its own process's memory map and per-thread CPU (the `memory_diagnostics` and
    /// `cpu_diagnostics` tools). No UI asks: it decides whether those tools exist on this OS.
    pub process_diagnostics: bool,
}
