//! Tests for the menu id ↔ command maps (`ids.rs`).

use super::*;

#[test]
fn test_menu_id_to_command_app_scoped() {
    // App-level commands should always emit
    assert_eq!(menu_id_to_command(ABOUT_ID), Some(("app.about", CommandScope::App)));
    assert_eq!(
        menu_id_to_command(SETTINGS_ID),
        Some(("app.settings", CommandScope::App))
    );
    assert_eq!(
        menu_id_to_command(ENTER_LICENSE_KEY_ID),
        Some(("app.licenseKey", CommandScope::App))
    );
    // Command palette is FileScoped: disabled when Settings/viewer has focus
    assert_eq!(
        menu_id_to_command(COMMAND_PALETTE_ID),
        Some(("app.commandPalette", CommandScope::FileScoped))
    );
}

#[test]
fn test_menu_id_to_command_file_scoped() {
    // File-scoped commands require main window focus
    assert_eq!(
        menu_id_to_command(SWITCH_PANE_ID),
        Some(("pane.switch", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(FILE_COPY_ID),
        Some(("file.copy", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(FILE_COMPRESS_ID),
        Some(("file.compress", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(GO_BACK_ID),
        Some(("nav.back", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(GO_TO_PATH_ID),
        Some(("nav.goToPath", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(GO_LATEST_DOWNLOAD_ID),
        Some(("downloads.goToLatest", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(SERVERS_CONNECT_ID),
        Some(("servers.connect", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(SERVERS_SHOW_ID),
        Some(("servers.show", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(NEW_TAB_ID),
        Some(("tab.new", CommandScope::FileScoped))
    );
    // Context menu items also routed through unified dispatch
    assert_eq!(
        menu_id_to_command(OPEN_ID),
        Some(("nav.open", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(EDIT_ID),
        Some(("file.edit", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(SHOW_IN_FINDER_ID),
        Some(("file.showInFinder", CommandScope::FileScoped))
    );
    // File-scoped, not App-scoped: `set_menu_context` only manages file-scoped
    // items, and this one has to grey out with the rest when Settings takes focus.
    assert_eq!(
        menu_id_to_command(OPEN_TERMINAL_HERE_ID),
        Some(("file.openTerminalHere", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(COPY_PATH_ID),
        Some(("file.copyPath", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(COPY_FILENAME_ID),
        Some(("file.copyFilename", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(GET_INFO_ID),
        Some(("file.getInfo", CommandScope::FileScoped))
    );
    assert_eq!(
        menu_id_to_command(QUICK_LOOK_ID),
        Some(("file.quickLook", CommandScope::FileScoped))
    );
}

#[test]
fn test_menu_id_to_command_unmapped() {
    // Items with special handling return None
    assert_eq!(menu_id_to_command(SHOW_HIDDEN_FILES_ID), None);
    assert_eq!(menu_id_to_command(VIEW_MODE_FULL_LEFT_ID), None);
    assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_LEFT_ID), None);
    assert_eq!(menu_id_to_command(VIEW_MODE_FULL_RIGHT_ID), None);
    assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_RIGHT_ID), None);
    assert_eq!(menu_id_to_command(VIEWER_WORD_WRAP_ID), None);
    // Sort order items (ascending/descending), date created, and date added use the
    // menu-sort event path and are not mapped. Only the four shortcut-bound columns are.
    assert_eq!(menu_id_to_command(SORT_ASCENDING_ID), None);
    assert_eq!(menu_id_to_command(SORT_DESCENDING_ID), None);
    assert_eq!(menu_id_to_command(SORT_BY_CREATED_ID), None);
    assert_eq!(menu_id_to_command(SORT_BY_ADDED_ID), None);
    assert_eq!(menu_id_to_command("unknown_id"), None);
}

#[test]
fn test_command_id_to_menu_id_roundtrip() {
    // Every entry in command_id_to_menu_id should map back correctly via menu_id_to_command
    let command_ids = [
        "app.about",
        "app.acknowledgements",
        "app.licenseKey",
        "app.settings",
        "app.commandPalette",
        "pane.switch",
        "pane.swap",
        "pane.clone",
        "view.calculateFolderSizes",
        "nav.back",
        "nav.forward",
        "nav.parent",
        "nav.goToPath",
        "downloads.goToLatest",
        "favorites.add",
        "favorites.open",
        "tab.new",
        "tab.close",
        "tab.reopen",
        "tab.next",
        "tab.prev",
        "tab.togglePin",
        "tab.closeOthers",
        "servers.connect",
        "servers.show",
        "search.open",
        "file.rename",
        "file.multiRename",
        "file.edit",
        "file.view",
        "file.copy",
        "file.move",
        "file.duplicate",
        "file.compress",
        "file.newFolder",
        "file.newFile",
        "file.delete",
        "file.deletePermanently",
        "file.showInFinder",
        "file.openTerminalHere",
        "file.copyPath",
        "file.copyFilename",
        "edit.undoTrash",
        "file.copyShareLink",
        "file.copyShareLinkOneDay",
        "file.copyShareLinkOneHour",
        "file.getInfo",
        "file.quickLook",
        "selection.selectAll",
        "selection.deselectAll",
        "selection.selectSameKind",
        "selection.compareDirectories",
        "selection.invert",
        "selection.selectFiles",
        "selection.deselectFiles",
        "help.openShortcuts",
        "help.viewDebugLog",
        "queue.show",
        "help.whatsNew",
        "log.operationLog",
        "suggestedOps.show",
        "help.sendErrorReport",
        "feedback.send",
        "app.checkForUpdates",
        "cmdr.openOnboarding",
        "cloud.makeOffline",
        "cloud.removeDownload",
        "sort.byName",
        "sort.byExtension",
        "sort.byModified",
        "sort.bySize",
    ];

    for command_id in &command_ids {
        let menu_id = command_id_to_menu_id(command_id);
        assert!(menu_id.is_some(), "command_id_to_menu_id missing: {command_id}");
        let (back, _scope) = menu_id_to_command(menu_id.unwrap())
            .unwrap_or_else(|| panic!("menu_id_to_command missing for menu_id from command {command_id}"));
        assert_eq!(back, *command_id, "roundtrip mismatch for {command_id}");
    }
}

#[test]
fn test_command_id_to_menu_id_unmapped() {
    // Commands without menu items return None
    assert_eq!(command_id_to_menu_id("view.fullMode"), None);
    assert_eq!(command_id_to_menu_id("view.briefMode"), None);
    assert_eq!(command_id_to_menu_id("view.showHidden"), None);
    assert_eq!(command_id_to_menu_id("unknown"), None);
}
