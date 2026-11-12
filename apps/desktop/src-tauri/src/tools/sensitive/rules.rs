//! The protected locations, for every OS's home folder at once.
//!
//! Every rule runs on every OS: a Mac browsing a NAS backup of a Windows profile must refuse
//! `AppData/Local/Google/Chrome` just as Windows does. What differs per OS is only where the
//! host keeps its own per-user dirs when it moved them, which `super::platform` answers.
//!
//! Where each location comes from, with its evidence: `../DETAILS.md` § The protected locations.

use super::Protected;
use Dir::{Anywhere, MacCaches, MacSupport, WinLocal, WinRoaming, XdgCache, XdgConfig, XdgData};
use Protected::{BrowserData, Credentials, MessageStore};

/// The per-user dir a rule sits in. Each has the spellings it takes inside a home folder, and
/// the host may add the absolute place it moved its own to (`$XDG_CONFIG_HOME`, `%APPDATA%`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    /// No parent: the rule's own parts are the anchor, at any depth.
    Anywhere,
    /// macOS `~/Library/Application Support`.
    MacSupport,
    /// macOS `~/Library/Caches`.
    MacCaches,
    /// `~/.config`, or `$XDG_CONFIG_HOME`.
    XdgConfig,
    /// `~/.local/share`, or `$XDG_DATA_HOME`.
    XdgData,
    /// `~/.cache`, or `$XDG_CACHE_HOME`.
    XdgCache,
    /// Windows `AppData\Roaming` (`%APPDATA%`).
    WinRoaming,
    /// Windows `AppData\Local` (`%LOCALAPPDATA%`).
    WinLocal,
}

impl Dir {
    /// Folded component runs that name this dir inside a home folder. The second Windows
    /// spelling is the hidden compatibility junction every profile carries, which opens the
    /// same folder (`Application Data` → `AppData\Roaming`).
    fn spellings(self) -> &'static [&'static [&'static str]] {
        match self {
            Anywhere => &[&[]],
            MacSupport => &[&["library", "application support"]],
            MacCaches => &[&["library", "caches"]],
            XdgConfig => &[&[".config"]],
            XdgData => &[&[".local", "share"]],
            XdgCache => &[&[".cache"]],
            WinRoaming => &[&["appdata", "roaming"], &["application data"]],
            WinLocal => &[&["appdata", "local"], &["local settings", "application data"]],
        }
    }
}

/// A run of folded path components, under any of `dirs`, that protects whatever sits at or
/// under it.
struct Rule {
    dirs: &'static [Dir],
    parts: &'static [&'static str],
    /// The last part also matches a longer name that continues with a separator: `chrome`
    /// covers `Chrome Beta`, `google-chrome` covers `google-chrome-unstable`.
    family: bool,
    reason: Protected,
}

const fn rule(dirs: &'static [Dir], parts: &'static [&'static str], reason: Protected) -> Rule {
    Rule {
        dirs,
        parts,
        family: false,
        reason,
    }
}

const fn family(dirs: &'static [Dir], parts: &'static [&'static str], reason: Protected) -> Rule {
    Rule {
        dirs,
        parts,
        family: true,
        reason,
    }
}

/// Folded, matched anywhere in the path, so a NAS backup of a home folder counts.
///
/// ❌ Never a bare `notes`, `edge`, `arc`, `brave`, `chrome`, `safari`, `opera`, `mozilla`, or
/// `firefox` under [`Anywhere`]: at any depth, case-folded, those would refuse
/// `~/Documents/notes/*`. Apps stay anchored under a [`Dir`] or a parent only they use.
const RULES: &[Rule] = &[
    // Keys and tool credentials. The dot folders sit in the home folder on every OS.
    rule(&[Anywhere], &[".ssh"], Credentials),
    rule(&[Anywhere], &[".gnupg"], Credentials),
    rule(&[WinRoaming], &["gnupg"], Credentials),
    rule(&[Anywhere], &[".password-store"], Credentials),
    rule(&[Anywhere], &[".aws"], Credentials),
    rule(&[Anywhere], &[".azure"], Credentials),
    rule(&[Anywhere], &[".kube"], Credentials),
    rule(&[XdgConfig, WinRoaming], &["gcloud"], Credentials),
    rule(&[XdgConfig], &["gh"], Credentials),
    rule(&[WinRoaming], &["github cli"], Credentials),
    rule(&[Anywhere], &[".docker", "config.json"], Credentials),
    // Tokens in remote URLs.
    rule(&[Anywhere], &[".git", "config"], Credentials),
    rule(&[Anywhere], &[".cargo", "credentials.toml"], Credentials),
    rule(&[Anywhere], &[".cargo", "credentials"], Credentials),
    rule(&[Anywhere], &[".claude", ".credentials.json"], Credentials),
    rule(&[Anywhere], &["sops", "age", "keys.txt"], Credentials),
    // The OS credential stores.
    rule(&[Anywhere], &["library", "keychains"], Credentials),
    rule(&[XdgData], &["keyrings"], Credentials),
    rule(&[XdgData], &["kwalletd"], Credentials),
    rule(&[WinRoaming, WinLocal], &["microsoft", "credentials"], Credentials),
    rule(&[WinRoaming, WinLocal], &["microsoft", "vault"], Credentials),
    rule(&[WinRoaming], &["microsoft", "protect"], Credentials),
    rule(&[WinRoaming], &["microsoft", "crypto"], Credentials),
    // Password managers.
    family(&[MacSupport, XdgConfig, WinRoaming], &["bitwarden"], Credentials),
    rule(
        &[Anywhere],
        &["library", "containers", "com.bitwarden.desktop"],
        Credentials,
    ),
    family(
        &[WinLocal],
        &["packages", "8bitsolutionsllc.bitwardendesktop"],
        Credentials,
    ),
    rule(&[Anywhere], &["snap", "bitwarden"], Credentials),
    rule(&[Anywhere], &[".var", "app", "com.bitwarden.desktop"], Credentials),
    rule(
        &[Anywhere],
        &["library", "group containers", "2bua8c4s2c.com.1password"],
        Credentials,
    ),
    rule(
        &[Anywhere],
        &["library", "group containers", "2bua8c4s2c.com.agilebits"],
        Credentials,
    ),
    rule(&[XdgConfig, WinLocal], &["1password"], Credentials),
    rule(
        &[Anywhere],
        &[".var", "app", "com.onepassword.onepassword"],
        Credentials,
    ),
    // Mail, messages, and notes.
    rule(&[Anywhere], &["library", "mail"], MessageStore),
    rule(&[Anywhere], &["library", "messages"], MessageStore),
    family(&[Anywhere], &["library", "containers", "com.apple.mail"], MessageStore),
    family(
        &[Anywhere],
        &["library", "containers", "com.apple.mobilesms"],
        MessageStore,
    ),
    family(&[Anywhere], &["library", "containers", "com.apple.notes"], MessageStore),
    family(
        &[Anywhere],
        &["library", "group containers", "group.com.apple.notes"],
        MessageStore,
    ),
    rule(&[Anywhere], &["library", "thunderbird"], MessageStore),
    rule(&[Anywhere], &[".thunderbird"], MessageStore),
    rule(
        &[MacCaches, XdgConfig, XdgCache, WinRoaming, WinLocal],
        &["thunderbird"],
        MessageStore,
    ),
    rule(&[Anywhere], &["snap", "thunderbird"], MessageStore),
    rule(&[Anywhere], &[".var", "app", "org.mozilla.thunderbird"], MessageStore),
    rule(
        &[Anywhere],
        &["library", "group containers", "ubf8t346g9.office", "outlook"],
        MessageStore,
    ),
    rule(&[WinLocal, WinRoaming], &["microsoft", "outlook"], MessageStore),
    rule(&[WinLocal], &["microsoft", "olk"], MessageStore),
    rule(&[Anywhere], &["documents", "outlook files"], MessageStore),
    rule(&[XdgData, XdgConfig, XdgCache], &["evolution"], MessageStore),
    rule(&[Anywhere], &[".var", "app", "org.gnome.evolution"], MessageStore),
    family(&[XdgData, XdgConfig, XdgCache], &["geary"], MessageStore),
    rule(&[Anywhere], &[".var", "app", "org.gnome.geary"], MessageStore),
    family(&[XdgData], &["local-mail"], MessageStore),
    family(&[XdgData], &[".local-mail"], MessageStore),
    rule(&[XdgData], &["akonadi"], MessageStore),
    // Browsers: Safari and the system cookie store.
    rule(&[Anywhere], &["library", "cookies"], BrowserData),
    rule(&[Anywhere], &["library", "safari"], BrowserData),
    family(&[Anywhere], &["library", "containers", "com.apple.safari"], BrowserData),
    rule(
        &[Anywhere],
        &["library", "containers", "com.apple.safaritechnologypreview"],
        BrowserData,
    ),
    family(&[MacCaches], &["com.apple.safari"], BrowserData),
    // Browsers: Firefox. `.mozilla` is the Linux home of releases before 147, `mozilla` under
    // the XDG dirs the one after.
    rule(&[Anywhere], &[".mozilla"], BrowserData),
    rule(&[XdgConfig, XdgCache, WinRoaming, WinLocal], &["mozilla"], BrowserData),
    rule(&[MacSupport, MacCaches], &["firefox"], BrowserData),
    family(&[WinLocal], &["packages", "mozilla.firefox"], BrowserData),
    rule(&[Anywhere], &["snap", "firefox"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "org.mozilla.firefox"], BrowserData),
    // Browsers: the Chromium family.
    family(&[MacSupport, MacCaches, WinLocal], &["google", "chrome"], BrowserData),
    family(&[XdgConfig, XdgCache], &["google-chrome"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "com.google.chrome"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "com.google.chromedev"], BrowserData),
    rule(
        &[MacSupport, MacCaches, XdgConfig, XdgCache, WinLocal],
        &["chromium"],
        BrowserData,
    ),
    rule(&[Anywhere], &["snap", "chromium"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "org.chromium.chromium"], BrowserData),
    rule(
        &[MacSupport, MacCaches, XdgConfig, XdgCache, WinLocal],
        &["bravesoftware"],
        BrowserData,
    ),
    rule(&[Anywhere], &["snap", "brave"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "com.brave.browser"], BrowserData),
    family(&[MacSupport, MacCaches], &["microsoft edge"], BrowserData),
    family(&[XdgConfig, XdgCache], &["microsoft-edge"], BrowserData),
    family(&[WinLocal], &["microsoft", "edge"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "com.microsoft.edge"], BrowserData),
    family(
        &[MacSupport, MacCaches, XdgConfig, XdgCache, WinLocal],
        &["vivaldi"],
        BrowserData,
    ),
    rule(&[Anywhere], &[".var", "app", "com.vivaldi.vivaldi"], BrowserData),
    family(&[MacSupport, MacCaches], &["com.operasoftware.opera"], BrowserData),
    rule(&[MacSupport, MacCaches], &["com.operasoftware.operagx"], BrowserData),
    family(&[XdgConfig, XdgCache], &["opera"], BrowserData),
    rule(&[WinRoaming, WinLocal], &["opera software"], BrowserData),
    rule(&[Anywhere], &[".var", "app", "com.opera.opera"], BrowserData),
    rule(&[MacSupport, MacCaches], &["arc"], BrowserData),
    family(&[WinLocal], &["packages", "thebrowsercompany.arc"], BrowserData),
];

/// Folded, matched against the last component only: a `.env` FOLDER is a Python venv.
const CREDENTIAL_FILE_NAMES: &[&str] = &[
    ".netrc",
    "_netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".pgpass",
    "pgpass.conf",
    ".vault-token",
    "rclone.conf",
];

/// What may follow a family rule's last part in a longer name of the same family.
const FAMILY_SEPARATORS: [char; 4] = [' ', '-', '.', '_'];

/// A per-user dir the host keeps somewhere else, as folded components.
pub(super) type Moved = (Dir, Vec<String>);

impl Rule {
    fn last_matches(&self, name: &str, part: &str) -> bool {
        match name.strip_prefix(part) {
            Some("") => true,
            Some(rest) => self.family && rest.starts_with(FAMILY_SEPARATORS),
            None => false,
        }
    }

    /// Whether `tail` starts with this rule's parts.
    fn starts(&self, tail: &[String]) -> bool {
        let Some((last, lead)) = self.parts.split_last() else {
            return false;
        };
        tail.len() >= self.parts.len()
            && tail.iter().zip(lead).all(|(have, want)| have == want)
            && self.last_matches(&tail[lead.len()], last)
    }

    fn covers(&self, parts: &[String], moved: &[Moved]) -> bool {
        self.dirs.iter().any(|dir| {
            let spelled = dir.spellings().iter().any(|spelling| {
                (0..parts.len()).any(|at| {
                    let tail = &parts[at..];
                    tail.len() > spelling.len()
                        && tail.iter().zip(*spelling).all(|(have, want)| have == want)
                        && self.starts(&tail[spelling.len()..])
                })
            });
            spelled
                || moved.iter().any(|(moved_dir, root)| {
                    moved_dir == dir && !root.is_empty() && parts.starts_with(root) && self.starts(&parts[root.len()..])
                })
        })
    }
}

/// Which rule protects a path of these folded components, if any. `moved` is where the host
/// keeps the per-user dirs it relocated.
pub(super) fn protecting(parts: &[String], moved: &[Moved]) -> Option<Protected> {
    if let Some(rule) = RULES.iter().find(|rule| rule.covers(parts, moved)) {
        return Some(rule.reason);
    }
    let name = parts.last()?;
    (name.starts_with(".env") || CREDENTIAL_FILE_NAMES.contains(&name.as_str())).then_some(Credentials)
}
