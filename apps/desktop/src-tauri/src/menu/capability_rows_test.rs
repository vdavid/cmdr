//! The menu bar and the frontend agree on which OS features exist. A bar row for an OS feature
//! (`cmdr_menu` can't read `PlatformCapabilities`: it has no `tauri`) is built exactly where this
//! OS's capability arm says the feature exists, so the menu never offers what the palette, the
//! shortcuts editor, and the settings hide. Each OS's test run checks its own arm.

use cmdr_menu::bar::spec_entry;
use cmdr_menu::ids::{GET_INFO_ID, OPEN_TERMINAL_HERE_ID, QUICK_LOOK_ID, QUIT_ID};
use cmdr_menu::spec::Platform;

use crate::platform::{PlatformCapabilities, capabilities};

/// Reads one capability off the OS's answer.
type CapabilityOf = fn(&PlatformCapabilities) -> bool;

/// Each OS-feature row, by menu id, with the capability that says the feature exists.
const CAPABILITY_ROWS: &[(&str, CapabilityOf)] = &[
    (GET_INFO_ID, |c| c.get_info),
    (QUICK_LOOK_ID, |c| c.quick_look),
    (OPEN_TERMINAL_HERE_ID, |c| c.open_terminal_here),
    // Cmdr's own Quit row exists exactly where no app menu owns Quit.
    (QUIT_ID, |c| !c.app_menu),
];

#[test]
fn each_os_feature_row_is_built_exactly_where_its_capability_holds() {
    let offered = capabilities();
    for (menu_id, holds) in CAPABILITY_ROWS {
        assert_eq!(
            spec_entry(menu_id, Platform::current()).is_some(),
            holds(&offered),
            "the menu row `{menu_id}` and its capability disagree on this OS",
        );
    }
}
