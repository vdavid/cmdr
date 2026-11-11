//! The app menu's "Install “cmdr” command": a symlink at `/usr/local/bin/cmdr` pointing at the
//! script inside this bundle, the way VS Code installs `code`.
//!
//! The state reads through to the filesystem every time, ❌ never a stored setting: the user can
//! delete the link, `brew install` the cask, or move Cmdr from a terminal, and a cached flag would
//! lie about all three. Everything that touches the machine sits behind [`CommandHost`] so the
//! state machine is testable without writing into `/usr/local/bin`.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The name the command goes by on PATH.
pub(crate) const COMMAND_NAME: &str = "cmdr";

/// Where the script sits inside a Cmdr bundle, relative to the `.app`. ❗ Must match the
/// `bundle.macOS.files` entry in `tauri.macos.conf.json` and the cask's `binary` stanza.
pub(crate) const BUNDLE_SCRIPT: &str = "Contents/Resources/bin/cmdr";

/// Every Cmdr's bundle id starts with this: the production one is exactly it, and dev, worktree,
/// and E2E instances add a `-<instance>` suffix (`apps/desktop/scripts/instance-id.ts`).
const CMDR_BUNDLE_ID_PREFIX: &str = "com.veszelovszki.cmdr";

/// The Homebrew cask's token. A cask install leaves `<prefix>/Caskroom/cmdr` behind, which is how
/// we tell its link from ours when both would sit in `/usr/local/bin` (Intel Macs).
const CASK_TOKEN: &str = "cmdr";

/// Where the command stands, read through to the filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CliCommandState {
    /// Nothing at `/usr/local/bin/cmdr`, and no Homebrew install either.
    NotInstalled,
    /// Our link: `/usr/local/bin/cmdr` points at a Cmdr bundle's script. Any Cmdr copy's, so a
    /// link made from an older location still reads as installed and can be removed.
    Installed,
    /// The Homebrew cask put `cmdr` on PATH. We leave it alone: `brew` owns that link, and
    /// uninstalling the cask removes it.
    InstalledByHomebrew,
    /// Something else is at `/usr/local/bin/cmdr`. ❌ Never touched: it's someone else's `cmdr`.
    TakenByOther,
}

/// Why this copy of Cmdr won't install the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CliCommandBlocker {
    /// A debug build or a dev / worktree / E2E instance (`reveal::registration::own_bundle_id`).
    /// Its link would point into a `target/` dir that comes and goes. Holds in both directions:
    /// such a build doesn't remove the installed copy's link either.
    NotProductionBuild,
    /// This copy isn't in an Applications folder, so it's one that gets moved or deleted, and the
    /// link would dangle (`crate::install_location`). Blocks installing only, never removing.
    NotInApplications,
}

/// What the menu item and the palette row show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CliCommandStatus {
    pub state: CliCommandState,
    /// Why a click on the item would be refused, or `None` when it would go ahead.
    pub blocked_by: Option<CliCommandBlocker>,
}

/// What an install or uninstall did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CliCommandOutcome {
    Installed,
    Uninstalled,
    /// It was already the way the click asked for.
    Unchanged,
    /// The user dismissed the administrator prompt.
    Canceled,
    /// Refused without touching anything. The returned status says why (its state or its
    /// `blocked_by`), so the frontend words it from one place.
    Refused,
    /// The OS said no. `code` is an errno or an AppleScript error number, for the log.
    Failed {
        code: Option<i32>,
    },
}

/// An outcome plus the state it left things in, so the caller renders the truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CliCommandResult {
    pub outcome: CliCommandOutcome,
    pub status: CliCommandStatus,
}

/// What `lstat` + `readlink` found at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathEntry {
    Missing,
    Symlink(PathBuf),
    /// A file, a folder, anything that isn't a symlink.
    Other,
}

/// How a privileged shell command went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdminOutcome {
    Done,
    Canceled,
    Failed(Option<i32>),
}

/// The machine, behind a seam.
pub(crate) trait CommandHost {
    fn entry(&self, path: &Path) -> PathEntry;
    fn exists(&self, path: &Path) -> bool;
    /// The `CFBundleIdentifier` in `<bundle>/Contents/Info.plist`, or `None` when unreadable.
    fn bundle_id(&self, bundle: &Path) -> Option<String>;
    /// `ln -s target link`, as this user. Must fail rather than replace an existing `link`.
    fn symlink(&self, target: &Path, link: &Path) -> io::Result<()>;
    /// Remove the link at `link`, as this user.
    fn remove_link(&self, link: &Path) -> io::Result<()>;
    /// Run `shell` as root behind macOS's administrator prompt, which shows `prompt`.
    fn run_as_admin(&self, shell: &str, prompt: &str) -> AdminOutcome;
}

/// Where things live. Fixed in production; tests point it at a scratch dir.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    /// Where our link goes: `/usr/local/bin`.
    pub install_dir: PathBuf,
    /// Homebrew prefixes to look for a cask install under: `/opt/homebrew`, `/usr/local`.
    pub brew_prefixes: Vec<PathBuf>,
}

impl Layout {
    pub(crate) fn system() -> Self {
        Self {
            install_dir: PathBuf::from("/usr/local/bin"),
            brew_prefixes: vec![PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")],
        }
    }

    fn link(&self) -> PathBuf {
        self.install_dir.join(COMMAND_NAME)
    }
}

/// The install state machine.
pub(crate) struct CliInstall<H: CommandHost> {
    host: H,
    layout: Layout,
    /// This copy's own script, or `None` when it's not a build that may install
    /// ([`CliCommandBlocker::NotProductionBuild`]).
    own_script: Option<PathBuf>,
    /// Where this copy sits, before the never-block-removal rule in [`Self::blocked_by`].
    install_blocker: Option<CliCommandBlocker>,
}

impl<H: CommandHost> CliInstall<H> {
    pub(crate) fn new(
        host: H,
        layout: Layout,
        own_script: Option<PathBuf>,
        install_blocker: Option<CliCommandBlocker>,
    ) -> Self {
        Self {
            host,
            layout,
            own_script,
            install_blocker,
        }
    }

    pub(crate) fn status(&self) -> CliCommandStatus {
        let state = self.state();
        CliCommandStatus {
            blocked_by: self.blocked_by(state),
            state,
        }
    }

    /// ❗ An installed command beats the install blocker: removing the link has to stay possible
    /// from wherever this copy has ended up, or a copy that moved out of Applications would
    /// strand a link it can't take back. A non-production build is blocked both ways; the link
    /// it would remove belongs to the installed copy.
    fn blocked_by(&self, state: CliCommandState) -> Option<CliCommandBlocker> {
        if self.own_script.is_none() {
            return Some(CliCommandBlocker::NotProductionBuild);
        }
        if state == CliCommandState::Installed {
            return None;
        }
        self.install_blocker
    }

    fn state(&self) -> CliCommandState {
        let link = self.layout.link();
        let ours = self.host.entry(&link);
        let ours_is_cmdr = matches!(&ours, PathEntry::Symlink(target) if self.is_cmdr_script(&link, target));
        if ours_is_cmdr && !self.cask_owns(&link) {
            return CliCommandState::Installed;
        }
        let by_homebrew = self.layout.brew_prefixes.iter().any(|prefix| {
            let brew_link = prefix.join("bin").join(COMMAND_NAME);
            self.cask_owns(&brew_link)
                && matches!(self.host.entry(&brew_link), PathEntry::Symlink(target) if self.is_cmdr_script(&brew_link, &target))
        });
        if by_homebrew {
            return CliCommandState::InstalledByHomebrew;
        }
        match ours {
            PathEntry::Missing => CliCommandState::NotInstalled,
            _ => CliCommandState::TakenByOther,
        }
    }

    /// Whether `link` sits in the `bin` of a Homebrew prefix that has the cask installed.
    fn cask_owns(&self, link: &Path) -> bool {
        self.layout.brew_prefixes.iter().any(|prefix| {
            link.parent() == Some(prefix.join("bin").as_path())
                && self.host.exists(&prefix.join("Caskroom").join(CASK_TOKEN))
        })
    }

    /// Whether a link's target is the script inside some Cmdr bundle. A bundle that's gone (the
    /// copy was moved or deleted) still counts: nothing else ships a script at that path, and its
    /// dangling link is exactly what the user needs to be able to remove.
    fn is_cmdr_script(&self, link: &Path, target: &Path) -> bool {
        let target = match link.parent() {
            Some(dir) if target.is_relative() => dir.join(target),
            _ => target.to_path_buf(),
        };
        let Some(bundle) = bundle_of_script(&target) else {
            return false;
        };
        self.host
            .bundle_id(&bundle)
            .is_none_or(|id| id.starts_with(CMDR_BUNDLE_ID_PREFIX))
    }

    /// Put the link in place, asking for the administrator password when the folder needs it.
    pub(crate) fn install(&self, prompt: &str) -> CliCommandResult {
        let status = self.status();
        let Some(script) = self.own_script.as_deref() else {
            return self.result(CliCommandOutcome::Refused);
        };
        if status.blocked_by.is_some() {
            return self.result(CliCommandOutcome::Refused);
        }
        match status.state {
            CliCommandState::Installed => return self.result(CliCommandOutcome::Unchanged),
            CliCommandState::InstalledByHomebrew | CliCommandState::TakenByOther => {
                return self.result(CliCommandOutcome::Refused);
            }
            CliCommandState::NotInstalled => {}
        }
        let link = self.layout.link();
        let outcome = match self.host.symlink(script, &link) {
            Ok(()) => CliCommandOutcome::Installed,
            // Root-owned (the default), or missing altogether (common on Apple Silicon, where
            // Homebrew lives in `/opt/homebrew` and nothing ever made `/usr/local/bin`).
            Err(err) if matches!(err.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound) => {
                let shell = format!(
                    "/bin/mkdir -p {} && /bin/ln -s {} {}",
                    shell_quote(&self.layout.install_dir),
                    shell_quote(script),
                    shell_quote(&link),
                );
                admin_outcome(self.host.run_as_admin(&shell, prompt), CliCommandOutcome::Installed)
            }
            Err(err) => CliCommandOutcome::Failed {
                code: err.raw_os_error(),
            },
        };
        self.result(outcome)
    }

    /// Remove our link, asking for the administrator password when the folder needs it. Only ever
    /// a link that points at a Cmdr script, never someone else's `cmdr`, never Homebrew's.
    pub(crate) fn uninstall(&self, prompt: &str) -> CliCommandResult {
        let status = self.status();
        if status.blocked_by.is_some() {
            return self.result(CliCommandOutcome::Refused);
        }
        match status.state {
            CliCommandState::NotInstalled => return self.result(CliCommandOutcome::Unchanged),
            CliCommandState::InstalledByHomebrew | CliCommandState::TakenByOther => {
                return self.result(CliCommandOutcome::Refused);
            }
            CliCommandState::Installed => {}
        }
        let link = self.layout.link();
        let PathEntry::Symlink(target) = self.host.entry(&link) else {
            return self.result(CliCommandOutcome::Refused);
        };
        let outcome = match self.host.remove_link(&link) {
            Ok(()) => CliCommandOutcome::Uninstalled,
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                // Re-checked as root, right before the `rm`, so a link swapped in after we looked
                // can't be what gets deleted.
                let shell = format!(
                    "[ \"$(/usr/bin/readlink {link})\" = {target} ] && /bin/rm {link}",
                    link = shell_quote(&link),
                    target = shell_quote(&target),
                );
                admin_outcome(self.host.run_as_admin(&shell, prompt), CliCommandOutcome::Uninstalled)
            }
            Err(err) => CliCommandOutcome::Failed {
                code: err.raw_os_error(),
            },
        };
        self.result(outcome)
    }

    fn result(&self, outcome: CliCommandOutcome) -> CliCommandResult {
        CliCommandResult {
            outcome,
            status: self.status(),
        }
    }
}

/// The `.app` a script path sits in, when it's at [`BUNDLE_SCRIPT`] inside one.
fn bundle_of_script(script: &Path) -> Option<PathBuf> {
    let text = script.to_str()?;
    let bundle = text.strip_suffix(BUNDLE_SCRIPT)?.strip_suffix('/')?;
    bundle.ends_with(".app").then(|| PathBuf::from(bundle))
}

fn admin_outcome(outcome: AdminOutcome, done: CliCommandOutcome) -> CliCommandOutcome {
    match outcome {
        AdminOutcome::Done => done,
        AdminOutcome::Canceled => CliCommandOutcome::Canceled,
        AdminOutcome::Failed(code) => CliCommandOutcome::Failed { code },
    }
}

/// Single-quote `path` for `/bin/sh`: every `'` becomes `'\''`, so no character in a path can
/// end the quoting early.
pub(crate) fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const SCRIPT: &str = "/Applications/Cmdr.app/Contents/Resources/bin/cmdr";

    /// An in-memory machine: a path → entry map, the bundle ids it knows, whether the install dir
    /// is writable without a prompt, and what the admin prompt answers.
    struct FakeHost {
        entries: RefCell<HashMap<PathBuf, PathEntry>>,
        dirs: Vec<PathBuf>,
        bundle_ids: HashMap<PathBuf, String>,
        writable: bool,
        admin_answer: AdminOutcome,
        admin_calls: RefCell<Vec<String>>,
    }

    impl FakeHost {
        fn new() -> Self {
            Self {
                entries: RefCell::new(HashMap::new()),
                dirs: Vec::new(),
                bundle_ids: HashMap::from([(
                    PathBuf::from("/Applications/Cmdr.app"),
                    CMDR_BUNDLE_ID_PREFIX.to_string(),
                )]),
                writable: false,
                admin_answer: AdminOutcome::Done,
                admin_calls: RefCell::new(Vec::new()),
            }
        }

        fn with_link(self, link: &str, target: &str) -> Self {
            self.entries
                .borrow_mut()
                .insert(PathBuf::from(link), PathEntry::Symlink(PathBuf::from(target)));
            self
        }

        fn with_file(self, path: &str) -> Self {
            self.entries.borrow_mut().insert(PathBuf::from(path), PathEntry::Other);
            self
        }

        fn with_cask_under(mut self, prefix: &str) -> Self {
            self.dirs.push(Path::new(prefix).join("Caskroom/cmdr"));
            self
        }

        fn link_at(&self, path: &str) -> PathEntry {
            self.entry(Path::new(path))
        }
    }

    impl CommandHost for FakeHost {
        fn entry(&self, path: &Path) -> PathEntry {
            self.entries.borrow().get(path).cloned().unwrap_or(PathEntry::Missing)
        }
        fn exists(&self, path: &Path) -> bool {
            self.dirs.iter().any(|dir| dir == path)
        }
        fn bundle_id(&self, bundle: &Path) -> Option<String> {
            self.bundle_ids.get(bundle).cloned()
        }
        fn symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
            if !self.writable {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            if self.entry(link) != PathEntry::Missing {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            self.entries
                .borrow_mut()
                .insert(link.to_path_buf(), PathEntry::Symlink(target.to_path_buf()));
            Ok(())
        }
        fn remove_link(&self, link: &Path) -> io::Result<()> {
            if !self.writable {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            self.entries.borrow_mut().remove(link);
            Ok(())
        }
        fn run_as_admin(&self, shell: &str, prompt: &str) -> AdminOutcome {
            self.admin_calls.borrow_mut().push(format!("{prompt} | {shell}"));
            if self.admin_answer == AdminOutcome::Done {
                // Play the privileged command's effect on the fake filesystem.
                let link = PathBuf::from("/usr/local/bin/cmdr");
                if shell.contains("/bin/ln -s") {
                    self.entries
                        .borrow_mut()
                        .insert(link, PathEntry::Symlink(PathBuf::from(SCRIPT)));
                } else if shell.contains("/bin/rm") {
                    self.entries.borrow_mut().remove(&link);
                }
            }
            self.admin_answer
        }
    }

    fn installer(host: FakeHost) -> CliInstall<FakeHost> {
        CliInstall::new(host, Layout::system(), Some(PathBuf::from(SCRIPT)), None)
    }

    fn status(state: CliCommandState, blocked_by: Option<CliCommandBlocker>) -> CliCommandStatus {
        CliCommandStatus { state, blocked_by }
    }

    #[test]
    fn shell_quoting_survives_a_quote_in_the_path() {
        assert_eq!(shell_quote(Path::new("/a b/it's")), r"'/a b/it'\''s'");
    }

    #[test]
    fn nothing_there_reads_as_not_installed() {
        assert_eq!(
            installer(FakeHost::new()).status(),
            status(CliCommandState::NotInstalled, None)
        );
    }

    #[test]
    fn a_link_to_any_cmdr_bundle_reads_as_installed() {
        let host = FakeHost::new().with_link("/usr/local/bin/cmdr", SCRIPT);
        assert_eq!(installer(host).status().state, CliCommandState::Installed);
        // A link made from a copy that has since moved or been deleted is still ours to remove.
        let host = FakeHost::new().with_link(
            "/usr/local/bin/cmdr",
            "/Users/jane/Downloads/Cmdr.app/Contents/Resources/bin/cmdr",
        );
        assert_eq!(installer(host).status().state, CliCommandState::Installed);
    }

    #[test]
    fn someone_elses_cmdr_is_taken_by_other() {
        let host = FakeHost::new().with_file("/usr/local/bin/cmdr");
        assert_eq!(installer(host).status().state, CliCommandState::TakenByOther);
        let host = FakeHost::new().with_link("/usr/local/bin/cmdr", "/opt/other-tool/bin/cmdr");
        assert_eq!(installer(host).status().state, CliCommandState::TakenByOther);
    }

    #[test]
    fn a_lookalike_path_in_another_apps_bundle_is_not_ours() {
        let mut host = FakeHost::new().with_link(
            "/usr/local/bin/cmdr",
            "/Applications/Other.app/Contents/Resources/bin/cmdr",
        );
        host.bundle_ids.insert(
            PathBuf::from("/Applications/Other.app"),
            "com.example.other".to_string(),
        );
        assert_eq!(installer(host).status().state, CliCommandState::TakenByOther);
    }

    #[test]
    fn a_cask_install_reads_as_homebrews_on_either_prefix() {
        let host = FakeHost::new()
            .with_cask_under("/opt/homebrew")
            .with_link("/opt/homebrew/bin/cmdr", SCRIPT);
        assert_eq!(installer(host).status().state, CliCommandState::InstalledByHomebrew);
        // Intel: brew's link sits exactly where ours would, so the Caskroom is what tells them apart.
        let host = FakeHost::new()
            .with_cask_under("/usr/local")
            .with_link("/usr/local/bin/cmdr", SCRIPT);
        assert_eq!(installer(host).status().state, CliCommandState::InstalledByHomebrew);
    }

    #[test]
    fn our_own_link_wins_over_an_apple_silicon_cask() {
        // Both on PATH: ours is the one this menu item made, so it's the one it can remove.
        let host = FakeHost::new()
            .with_cask_under("/opt/homebrew")
            .with_link("/opt/homebrew/bin/cmdr", SCRIPT)
            .with_link("/usr/local/bin/cmdr", SCRIPT);
        assert_eq!(installer(host).status().state, CliCommandState::Installed);
    }

    #[test]
    fn a_cask_without_a_link_doesnt_count() {
        // An older cask had no `binary` stanza, so it left nothing on PATH.
        let host = FakeHost::new().with_cask_under("/opt/homebrew");
        assert_eq!(installer(host).status().state, CliCommandState::NotInstalled);
    }

    #[test]
    fn a_dev_build_is_blocked_both_ways() {
        let host = FakeHost::new().with_link("/usr/local/bin/cmdr", SCRIPT);
        let dev = CliInstall::new(host, Layout::system(), None, None);
        assert_eq!(
            dev.status(),
            status(CliCommandState::Installed, Some(CliCommandBlocker::NotProductionBuild))
        );
        let result = dev.uninstall("prompt");
        assert_eq!(result.outcome, CliCommandOutcome::Refused);
        assert_eq!(
            dev.host.link_at("/usr/local/bin/cmdr"),
            PathEntry::Symlink(PathBuf::from(SCRIPT))
        );
        assert!(dev.host.admin_calls.borrow().is_empty());
    }

    #[test]
    fn outside_applications_blocks_installing_but_never_removing() {
        let blocked = |host| {
            CliInstall::new(
                host,
                Layout::system(),
                Some(PathBuf::from(SCRIPT)),
                Some(CliCommandBlocker::NotInApplications),
            )
        };
        let fresh = blocked(FakeHost::new());
        assert_eq!(fresh.status().blocked_by, Some(CliCommandBlocker::NotInApplications));
        assert_eq!(fresh.install("prompt").outcome, CliCommandOutcome::Refused);
        assert!(fresh.host.admin_calls.borrow().is_empty());

        let installed = blocked(FakeHost::new().with_link("/usr/local/bin/cmdr", SCRIPT));
        assert_eq!(installed.status().blocked_by, None);
        assert_eq!(installed.uninstall("prompt").outcome, CliCommandOutcome::Uninstalled);
    }

    #[test]
    fn installing_into_a_writable_dir_needs_no_prompt() {
        let mut host = FakeHost::new();
        host.writable = true;
        let cli = installer(host);
        let result = cli.install("prompt");
        assert_eq!(result.outcome, CliCommandOutcome::Installed);
        assert_eq!(result.status.state, CliCommandState::Installed);
        assert!(cli.host.admin_calls.borrow().is_empty());
    }

    #[test]
    fn installing_into_a_root_owned_dir_asks_for_the_admin_password() {
        let cli = installer(FakeHost::new());
        let result = cli.install("Cmdr wants to install");
        assert_eq!(result.outcome, CliCommandOutcome::Installed);
        assert_eq!(result.status.state, CliCommandState::Installed);
        let calls = cli.host.admin_calls.borrow();
        assert_eq!(calls.len(), 1);
        // `mkdir -p` because `/usr/local/bin` doesn't exist on a fresh Apple Silicon Mac, and a
        // plain `ln -s` (never `-f`) so whatever appeared there meanwhile is left alone.
        assert_eq!(
            calls[0],
            "Cmdr wants to install | /bin/mkdir -p '/usr/local/bin' && /bin/ln -s '/Applications/Cmdr.app/Contents/Resources/bin/cmdr' '/usr/local/bin/cmdr'"
        );
    }

    #[test]
    fn a_canceled_prompt_changes_nothing() {
        let mut host = FakeHost::new();
        host.admin_answer = AdminOutcome::Canceled;
        let cli = installer(host);
        let result = cli.install("prompt");
        assert_eq!(result.outcome, CliCommandOutcome::Canceled);
        assert_eq!(result.status.state, CliCommandState::NotInstalled);
    }

    #[test]
    fn a_failed_prompt_reports_its_code() {
        let mut host = FakeHost::new();
        host.admin_answer = AdminOutcome::Failed(Some(1));
        let result = installer(host).install("prompt");
        assert_eq!(result.outcome, CliCommandOutcome::Failed { code: Some(1) });
    }

    #[test]
    fn install_refuses_homebrew_and_someone_elses_cmdr() {
        let brew = installer(
            FakeHost::new()
                .with_cask_under("/opt/homebrew")
                .with_link("/opt/homebrew/bin/cmdr", SCRIPT),
        );
        assert_eq!(brew.install("prompt").outcome, CliCommandOutcome::Refused);
        let other = installer(FakeHost::new().with_file("/usr/local/bin/cmdr"));
        let result = other.install("prompt");
        assert_eq!(result.outcome, CliCommandOutcome::Refused);
        assert_eq!(result.status.state, CliCommandState::TakenByOther);
        assert!(other.host.admin_calls.borrow().is_empty());
    }

    #[test]
    fn installing_twice_is_unchanged() {
        let host = FakeHost::new().with_link("/usr/local/bin/cmdr", SCRIPT);
        assert_eq!(installer(host).install("prompt").outcome, CliCommandOutcome::Unchanged);
    }

    #[test]
    fn uninstall_removes_only_our_link() {
        let cli = installer(FakeHost::new().with_link("/usr/local/bin/cmdr", SCRIPT));
        let result = cli.uninstall("Cmdr wants to remove");
        assert_eq!(result.outcome, CliCommandOutcome::Uninstalled);
        assert_eq!(result.status.state, CliCommandState::NotInstalled);
        // The privileged step re-checks the link before removing it, so a swap in between
        // can't trick it into deleting someone else's `cmdr`.
        assert_eq!(
            cli.host.admin_calls.borrow()[0],
            "Cmdr wants to remove | [ \"$(/usr/bin/readlink '/usr/local/bin/cmdr')\" = '/Applications/Cmdr.app/Contents/Resources/bin/cmdr' ] && /bin/rm '/usr/local/bin/cmdr'"
        );

        let other = installer(FakeHost::new().with_file("/usr/local/bin/cmdr"));
        assert_eq!(other.uninstall("prompt").outcome, CliCommandOutcome::Refused);
        assert_eq!(other.host.link_at("/usr/local/bin/cmdr"), PathEntry::Other);

        let brew = installer(
            FakeHost::new()
                .with_cask_under("/usr/local")
                .with_link("/usr/local/bin/cmdr", SCRIPT),
        );
        assert_eq!(brew.uninstall("prompt").outcome, CliCommandOutcome::Refused);
        assert!(brew.host.admin_calls.borrow().is_empty(), "brew owns that link");
    }

    #[test]
    fn uninstalling_when_nothing_is_there_is_unchanged() {
        assert_eq!(
            installer(FakeHost::new()).uninstall("prompt").outcome,
            CliCommandOutcome::Unchanged
        );
    }

    #[test]
    fn a_writable_dir_uninstalls_without_a_prompt() {
        let mut host = FakeHost::new().with_link("/usr/local/bin/cmdr", SCRIPT);
        host.writable = true;
        let cli = installer(host);
        assert_eq!(cli.uninstall("prompt").outcome, CliCommandOutcome::Uninstalled);
        assert!(cli.host.admin_calls.borrow().is_empty());
    }
}
