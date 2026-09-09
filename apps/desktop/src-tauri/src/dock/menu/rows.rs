//! What the Dock tile menu says, decided over plain data.
//!
//! Everything interesting about the menu (which rows exist, in what order, what
//! each is called, and which of them are dropped as duplicates) is settled here,
//! with no AppKit anywhere near it. `native.rs` turns the answer into an `NSMenu`
//! and does nothing else worth testing.
//!
//! The one rule the whole file exists to serve: ❌ **never a syscall**. A path is
//! judged by its SHAPE alone, never by asking the filesystem about it, because a
//! favorite pointing at a sleeping NAS would hang the Dock for minutes while the
//! user holds the mouse down. A row naming a folder that has since gone away is a
//! click that lands on the pane's ordinary "couldn't open that" path, which is a
//! far better outcome than a beachball.

use std::path::Path;

/// How many bookmarks, and how many tabs, the menu is willing to draw.
///
/// A cap rather than a budget: favorites and tabs are both user-curated and rarely
/// reach double digits, but nothing stops someone keeping 200 favorites, and a Dock
/// menu that runs off the screen is worse than one that stops early.
pub const MAX_LOCATIONS_PER_GROUP: usize = 12;

/// A location offered to the menu, before dedup, disambiguation, and the cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The label the source already carries: a favorite's user-set name. `None`
    /// for a tab, which has no name of its own and takes the path's last component.
    pub name: Option<String>,
    /// The path exactly as the source stores it, `~` included.
    pub path: String,
}

/// Constructors for the tests. Production builds these in `sources.rs`, straight out
/// of a favorite or a `TabInfo`, where a struct literal is clearer than a helper.
#[cfg(test)]
impl Candidate {
    /// A tab: no name of its own.
    pub fn tab(path: impl Into<String>) -> Self {
        Self {
            name: None,
            path: path.into(),
        }
    }

    /// A favorite: the user's own label, and where it points.
    pub fn bookmark(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            path: path.into(),
        }
    }
}

/// One of the four fixed commands at the top of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockCommand {
    /// Bring the main window forward. The only row that acts entirely in Rust.
    OpenCmdr,
    /// Open the search dialog (`search.open`).
    SearchFiles,
    /// Open the go-to-path dialog (`nav.goToPath`).
    GoToFolder,
    /// Open the connect-to-server flow (`servers.connect`).
    ConnectToServer,
}

impl DockCommand {
    /// The four, in the order the menu draws them, and the single source of that
    /// order: [`menu_rows`] walks this rather than repeating the list.
    pub const ALL: [DockCommand; 4] = [
        DockCommand::OpenCmdr,
        DockCommand::SearchFiles,
        DockCommand::GoToFolder,
        DockCommand::ConnectToServer,
    ];

    /// The first command below the separator that splits the four into "what you
    /// wanted from the Dock" and "the two that open a dialog".
    const FIRST_BELOW_THE_RULE: DockCommand = DockCommand::GoToFolder;

    /// The registry command id a click on this row asks the frontend to run, or `None`
    /// for the one row Rust handles entirely by itself.
    ///
    /// ⚠️ These strings are the third place Rust names a frontend command id, after
    /// `menu/command_map.rs` and `settings/sections/LicenseSection.svelte`.
    /// `rust-command-id-drift.test.ts` scans all three; a rename that misses this file
    /// fails there.
    pub fn command_id(self) -> Option<&'static str> {
        match self {
            DockCommand::OpenCmdr => None,
            DockCommand::SearchFiles => Some("search.open"),
            DockCommand::GoToFolder => Some("nav.goToPath"),
            DockCommand::ConnectToServer => Some("servers.connect"),
        }
    }

    /// This row's `menu.*` catalog key. Resolved through `menu_t` at build time, so
    /// the menu speaks whatever the user reads.
    pub fn label_key(self) -> &'static str {
        match self {
            DockCommand::OpenCmdr => "menu.dock.openCmdr",
            DockCommand::SearchFiles => "menu.dock.searchFiles",
            DockCommand::GoToFolder => "menu.dock.goToFolder",
            DockCommand::ConnectToServer => "menu.dock.connectToServer",
        }
    }

    /// The SF Symbol drawn beside it. `magnifyingglass` and `arrow.right.to.line`
    /// are what the menu bar already gives the same two commands: the same concept
    /// gets the same glyph.
    pub fn symbol(self) -> &'static str {
        match self {
            DockCommand::OpenCmdr => "macwindow",
            DockCommand::SearchFiles => "magnifyingglass",
            DockCommand::GoToFolder => "arrow.right.to.line",
            DockCommand::ConnectToServer => "externaldrive.connected.to.line.below",
        }
    }
}

/// What a location row is called, structured rather than rendered.
///
/// Structured because the qualified form has to be worded by a translator
/// (`menu_t_with`), and because a test that asserts "this row got parent-qualified"
/// is asserting the decision rather than one language's punctuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockLabel {
    /// The row's own name. Nothing else in this menu is called that.
    Plain(String),
    /// The name, plus the folder holding it, because another row is called the same.
    InParent { name: String, parent: String },
    /// The whole path, because even the parent didn't tell the two apart. Paths are
    /// unique by the time this is reached, so it always does.
    Path(String),
}

impl DockLabel {
    /// The row's own name, whatever form the label took.
    fn name(&self) -> &str {
        match self {
            DockLabel::Plain(name) | DockLabel::Path(name) => name,
            DockLabel::InParent { name, .. } => name,
        }
    }

    /// What two labels have to share to still be indistinguishable after one pass.
    fn collision_key(&self) -> (&str, Option<&str>) {
        match self {
            DockLabel::Plain(name) | DockLabel::Path(name) => (name, None),
            DockLabel::InParent { name, parent } => (name, Some(parent)),
        }
    }
}

/// Which list a location row came from. Decides its glyph, nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationKind {
    /// A favorite.
    Bookmark,
    /// A tab one of the panes has open.
    Tab,
}

impl LocationKind {
    /// The SF Symbol drawn beside rows of this kind.
    pub fn symbol(self) -> &'static str {
        match self {
            LocationKind::Bookmark => "star",
            LocationKind::Tab => "folder",
        }
    }
}

/// A folder row: one bookmark, or one open tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockLocation {
    /// What the row is called, after disambiguation.
    pub label: DockLabel,
    /// The absolute path a click navigates to, `~` already expanded.
    pub path: String,
    /// Which list it came from.
    pub kind: LocationKind,
}

/// One row of the menu, in the order AppKit draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockRow {
    /// One of the four fixed commands.
    Command(DockCommand),
    /// A dividing line. Only ever emitted in front of a group that has rows.
    Separator,
    /// A bookmark or a tab.
    Location(DockLocation),
}

/// The whole menu, top to bottom.
///
/// macOS appends `Options ▸`, `Show All Windows`, and `Quit` below whatever this
/// returns; those stay, deliberately, because `Show All Windows` is how someone
/// reaches a particular file-viewer window.
///
/// `home` expands the `~` a persisted tab path carries; `None` (no home directory
/// to reason about) simply drops those rows.
pub fn menu_rows(bookmarks: &[Candidate], tabs: &[Candidate], home: Option<&Path>) -> Vec<DockRow> {
    let mut rows: Vec<DockRow> = Vec::with_capacity(DockCommand::ALL.len() + 1);
    for command in DockCommand::ALL {
        if command == DockCommand::FIRST_BELOW_THE_RULE {
            rows.push(DockRow::Separator);
        }
        rows.push(DockRow::Command(command));
    }

    // One `seen` list across both groups, so a tab pointing at a folder a bookmark
    // already offers is dropped rather than drawn twice. Finder's own Dock menu
    // lists "Applications" twice with nothing to tell the two apart; not copying
    // that is the point of this whole function.
    let mut seen: Vec<String> = Vec::new();
    let mut locations = take_locations(bookmarks, LocationKind::Bookmark, home, &mut seen);
    let bookmark_count = locations.len();
    locations.extend(take_locations(tabs, LocationKind::Tab, home, &mut seen));

    disambiguate(&mut locations);

    let mut tail = locations.into_iter();
    let bookmark_rows: Vec<DockLocation> = tail.by_ref().take(bookmark_count).collect();
    let tab_rows: Vec<DockLocation> = tail.collect();

    for group in [bookmark_rows, tab_rows] {
        if group.is_empty() {
            continue;
        }
        rows.push(DockRow::Separator);
        rows.extend(group.into_iter().map(DockRow::Location));
    }

    rows
}

/// The rows one group contributes: menu-able, not already offered, capped.
fn take_locations(
    candidates: &[Candidate],
    kind: LocationKind,
    home: Option<&Path>,
    seen: &mut Vec<String>,
) -> Vec<DockLocation> {
    let mut rows = Vec::new();
    for candidate in candidates {
        if rows.len() >= MAX_LOCATIONS_PER_GROUP {
            break;
        }
        let Some(path) = menuable_path(&candidate.path, home) else {
            continue;
        };
        let key = normalize_for_dedup(&path);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        rows.push(DockLocation {
            label: DockLabel::Plain(initial_name(candidate, &path)),
            path,
            kind,
        });
    }
    rows
}

/// The absolute path this candidate names, or `None` if the menu can't offer it.
///
/// Shape only, ❌ never a `stat`. Accepts an absolute path and the `~` form a pane
/// persists while it sits in the home folder; drops everything else, which is the
/// virtual locations (`search-results`, an MTP device, a share that isn't mounted)
/// no Dock click could navigate to anyway.
fn menuable_path(raw: &str, home: Option<&Path>) -> Option<String> {
    if raw.starts_with('/') {
        return Some(raw.to_string());
    }
    let home = home?;
    if raw == "~" {
        return Some(home.to_string_lossy().into_owned());
    }
    let rest = raw.strip_prefix("~/")?;
    Some(home.join(rest).to_string_lossy().into_owned())
}

/// The comparable form of a path: one trailing separator gone, but never the root's.
/// Mirrors `favorites::store`'s rule, including its case-sensitivity limitation (on
/// case-insensitive APFS `/Foo` and `/foo` compare unequal, worst case a row that
/// looks like a duplicate).
fn normalize_for_dedup(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// What a row is called before anything collides: the favorite's own label, else
/// the path's last component, else the path itself (which is what `/` gets).
fn initial_name(candidate: &Candidate, path: &str) -> String {
    if let Some(name) = candidate.name.as_deref().filter(|name| !name.is_empty()) {
        return name.to_string();
    }
    leaf(path).unwrap_or_else(|| path.to_string())
}

/// A path's last component, if it has one.
fn leaf(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

/// The name of the folder holding `path`, if there is one above the root.
fn parent_name(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

/// Gives every row a label nothing else in the menu carries.
///
/// Two passes, weakest change first: qualify a repeated name with the folder holding
/// it (`Projects (Documents)`), and fall back to the whole path for anything that
/// still matches. Paths are unique by now, so the second pass always settles it.
fn disambiguate(locations: &mut [DockLocation]) {
    let repeated = repeated_keys(locations.iter().map(|row| (row.label.name(), None)));
    for row in locations.iter_mut() {
        if !repeated.contains(&(row.label.name().to_string(), None)) {
            continue;
        }
        if let Some(parent) = parent_name(&row.path) {
            row.label = DockLabel::InParent {
                name: row.label.name().to_string(),
                parent,
            };
        }
    }

    let repeated = repeated_keys(locations.iter().map(|row| row.label.collision_key()));
    for row in locations.iter_mut() {
        let (name, parent) = row.label.collision_key();
        if repeated.contains(&(name.to_string(), parent.map(str::to_string))) {
            row.label = DockLabel::Path(row.path.clone());
        }
    }
}

/// The keys more than one row carries.
fn repeated_keys<'a>(keys: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> Vec<(String, Option<String>)> {
    let owned: Vec<(String, Option<String>)> = keys
        .map(|(name, parent)| (name.to_string(), parent.map(str::to_string)))
        .collect();
    let mut repeated: Vec<(String, Option<String>)> = Vec::new();
    for key in &owned {
        if owned.iter().filter(|other| *other == key).count() > 1 && !repeated.contains(key) {
            repeated.push(key.clone());
        }
    }
    repeated
}

#[cfg(test)]
mod tests;
