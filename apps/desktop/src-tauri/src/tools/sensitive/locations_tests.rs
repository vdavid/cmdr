//! Each OS's real locations against the rules, and every spelling a tool can be handed for
//! them. The promise under test: a path under a protected location is refused, however it's
//! written, on whichever OS is looking at it.

use std::path::PathBuf;

use super::Protected::{BrowserData, Credentials, MessageStore};
use super::*;
use crate::test_support::TestDir;

type Cell = (&'static str, Protected);

const MAC_HOME: &str = "/Users/someone";
const LINUX_HOME: &str = "/home/someone";
const WINDOWS_HOME: &str = "C:/Users/someone";

/// Paths under a macOS home folder.
const MAC: &[Cell] = &[
    ("Library/Keychains/login.keychain-db", Credentials),
    ("Library/Application Support/Bitwarden/data.json", Credentials),
    ("Library/Application Support/Bitwarden CLI/data.json", Credentials),
    (
        "Library/Containers/com.bitwarden.desktop/Data/Library/Application Support/Bitwarden/data.json",
        Credentials,
    ),
    (
        "Library/Group Containers/2BUA8C4S2C.com.1password/Library/Application Support/1Password/Data/1password.sqlite",
        Credentials,
    ),
    ("Library/Group Containers/2BUA8C4S2C.com.agilebits/x", Credentials),
    (".config/gh/hosts.yml", Credentials),
    (".config/gcloud/credentials.db", Credentials),
    ("Library/Application Support/sops/age/keys.txt", Credentials),
    (".cargo/credentials", Credentials),
    ("Library/Mail/V10/x.emlx", MessageStore),
    (
        "Library/Containers/com.apple.mail/Data/Library/Mail Downloads/a.pdf",
        MessageStore,
    ),
    (
        "Library/Containers/com.apple.mail.MailQuickLookExtension/Data/x",
        MessageStore,
    ),
    ("Library/Messages/chat.db", MessageStore),
    ("Library/Containers/com.apple.MobileSMS/Data/x", MessageStore),
    ("Library/Containers/com.apple.Notes/Data/x", MessageStore),
    (
        "Library/Group Containers/group.com.apple.notes/NoteStore.sqlite",
        MessageStore,
    ),
    ("Library/Group Containers/group.com.apple.notes.import/x", MessageStore),
    (
        "Library/Thunderbird/Profiles/a.default-release/ImapMail/x",
        MessageStore,
    ),
    (
        "Library/Caches/Thunderbird/Profiles/a.default-release/cache2/x",
        MessageStore,
    ),
    (
        "Library/Group Containers/UBF8T346G9.Office/Outlook/Outlook 15 Profiles/Main Profile/Data/Outlook.sqlite",
        MessageStore,
    ),
    ("Library/Safari/History.db", BrowserData),
    ("Library/Cookies/Cookies.binarycookies", BrowserData),
    (
        "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
        BrowserData,
    ),
    ("Library/Containers/com.apple.Safari/Data/Library/WebKit/x", BrowserData),
    ("Library/Containers/com.apple.Safari.WebApp/Data/x", BrowserData),
    (
        "Library/Containers/com.apple.SafariTechnologyPreview/Data/x",
        BrowserData,
    ),
    ("Library/Caches/com.apple.Safari/Cache.db", BrowserData),
    ("Library/Caches/com.apple.Safari.SafeBrowsing/x", BrowserData),
    (
        "Library/Application Support/Google/Chrome/Default/Login Data",
        BrowserData,
    ),
    (
        "Library/Application Support/Google/Chrome Beta/Default/Cookies",
        BrowserData,
    ),
    (
        "Library/Application Support/Google/Chrome Canary/Default/Cookies",
        BrowserData,
    ),
    (
        "Library/Application Support/Google/Chrome for Testing/Default/Cookies",
        BrowserData,
    ),
    ("Library/Caches/Google/Chrome/Default/Cache/Cache_Data/x", BrowserData),
    ("Library/Application Support/Chromium/Default/Cookies", BrowserData),
    (
        "Library/Application Support/BraveSoftware/Brave-Browser/Default/Cookies",
        BrowserData,
    ),
    (
        "Library/Application Support/Microsoft Edge/Default/Cookies",
        BrowserData,
    ),
    (
        "Library/Application Support/Microsoft Edge Beta/Default/Cookies",
        BrowserData,
    ),
    ("Library/Caches/Microsoft Edge/Default/Cache/x", BrowserData),
    ("Library/Application Support/Arc/User Data/Default/Cookies", BrowserData),
    (
        "Library/Application Support/Firefox/Profiles/a.default-release/logins.json",
        BrowserData,
    ),
    (
        "Library/Caches/Firefox/Profiles/a.default-release/cache2/x",
        BrowserData,
    ),
    ("Library/Application Support/Vivaldi/Default/Cookies", BrowserData),
    (
        "Library/Application Support/com.operasoftware.Opera/Default/Cookies",
        BrowserData,
    ),
    (
        "Library/Application Support/com.operasoftware.OperaGX/Default/Cookies",
        BrowserData,
    ),
];

/// Paths under a Linux home folder.
const LINUX: &[Cell] = &[
    (".ssh/id_ed25519", Credentials),
    (".gnupg/private-keys-v1.d/a.key", Credentials),
    (".local/share/keyrings/login.keyring", Credentials),
    (".local/share/kwalletd/kdewallet.kwl", Credentials),
    (".config/Bitwarden/data.json", Credentials),
    (".config/Bitwarden CLI/data.json", Credentials),
    ("snap/bitwarden/current/.config/Bitwarden/data.json", Credentials),
    (".var/app/com.bitwarden.desktop/config/Bitwarden/data.json", Credentials),
    (".config/1Password/1password.sqlite", Credentials),
    (".var/app/com.onepassword.OnePassword/config/1Password/x", Credentials),
    (
        ".thunderbird/a.default-release/ImapMail/imap.example.org/INBOX",
        MessageStore,
    ),
    (".config/thunderbird/a.default-release/x", MessageStore),
    (".cache/thunderbird/a.default-release/cache2/x", MessageStore),
    (
        "snap/thunderbird/common/.thunderbird/a.default/Mail/Inbox",
        MessageStore,
    ),
    (
        ".var/app/org.mozilla.Thunderbird/.thunderbird/a.default/Mail/Inbox",
        MessageStore,
    ),
    (".local/share/evolution/mail/local/cur/1", MessageStore),
    (".config/evolution/sources/a.source", MessageStore),
    (".cache/evolution/mail/a/folders.db", MessageStore),
    (".var/app/org.gnome.Evolution/data/evolution/mail/x", MessageStore),
    (".local/share/geary/account_01/geary.db", MessageStore),
    (".config/geary/account_01/geary.ini", MessageStore),
    (".local/share/geary-development/account_01/geary.db", MessageStore),
    (".var/app/org.gnome.Geary/data/geary/account_01/geary.db", MessageStore),
    (".local/share/local-mail/inbox/cur/1", MessageStore),
    (".local/share/.local-mail.directory/x", MessageStore),
    (".local/share/akonadi/file_db_data/1_r0", MessageStore),
    (".mozilla/firefox/a.default-release/logins.json", BrowserData),
    (".config/mozilla/firefox/a.default-release/logins.json", BrowserData),
    (".cache/mozilla/firefox/a.default-release/cache2/x", BrowserData),
    (
        "snap/firefox/common/.mozilla/firefox/a.default/cookies.sqlite",
        BrowserData,
    ),
    (
        ".var/app/org.mozilla.firefox/.mozilla/firefox/a.default/cookies.sqlite",
        BrowserData,
    ),
    (".config/google-chrome/Default/Login Data", BrowserData),
    (".config/google-chrome-beta/Default/Cookies", BrowserData),
    (".config/google-chrome-unstable/Default/Cookies", BrowserData),
    (".cache/google-chrome/Default/Cache/x", BrowserData),
    (
        ".var/app/com.google.Chrome/config/google-chrome/Default/Cookies",
        BrowserData,
    ),
    (".var/app/com.google.ChromeDev/config/x", BrowserData),
    (".config/chromium/Default/Cookies", BrowserData),
    ("snap/chromium/common/chromium/Default/Cookies", BrowserData),
    (
        ".var/app/org.chromium.Chromium/config/chromium/Default/Cookies",
        BrowserData,
    ),
    (".config/BraveSoftware/Brave-Browser/Default/Cookies", BrowserData),
    ("snap/brave/current/.config/BraveSoftware/x", BrowserData),
    (".var/app/com.brave.Browser/config/BraveSoftware/x", BrowserData),
    (".config/microsoft-edge/Default/Cookies", BrowserData),
    (".config/microsoft-edge-beta/Default/Cookies", BrowserData),
    (".var/app/com.microsoft.Edge/config/microsoft-edge/x", BrowserData),
    (".config/vivaldi/Default/Cookies", BrowserData),
    (".config/vivaldi-snapshot/Default/Cookies", BrowserData),
    (".var/app/com.vivaldi.Vivaldi/config/vivaldi/x", BrowserData),
    (".config/opera/Cookies", BrowserData),
    (".config/opera-beta/Cookies", BrowserData),
    (".var/app/com.opera.Opera/config/opera/x", BrowserData),
];

/// Paths under a Windows user folder, in the app's slash form.
const WINDOWS: &[Cell] = &[
    (".ssh/id_ed25519", Credentials),
    (".aws/credentials", Credentials),
    (".azure/msal_token_cache.json", Credentials),
    (".kube/config", Credentials),
    (".docker/config.json", Credentials),
    ("_netrc", Credentials),
    ("AppData/Roaming/gnupg/private-keys-v1.d/a.key", Credentials),
    ("AppData/Roaming/gcloud/credentials.db", Credentials),
    ("AppData/Roaming/GitHub CLI/hosts.yml", Credentials),
    ("AppData/Roaming/postgresql/pgpass.conf", Credentials),
    ("AppData/Roaming/rclone/rclone.conf", Credentials),
    ("AppData/Roaming/sops/age/keys.txt", Credentials),
    (
        "AppData/Roaming/Microsoft/Credentials/DFBE70A7E5CC19A398EBF1B96859CE5D",
        Credentials,
    ),
    (
        "AppData/Local/Microsoft/Credentials/DFBE70A7E5CC19A398EBF1B96859CE5D",
        Credentials,
    ),
    (
        "AppData/Local/Microsoft/Vault/4BF4C442-9B8A-41A0-B380-DD4A704DDB28/Policy.vpol",
        Credentials,
    ),
    ("AppData/Roaming/Microsoft/Vault/x", Credentials),
    ("AppData/Roaming/Microsoft/Protect/S-1-5-21-1/a-master-key", Credentials),
    ("AppData/Roaming/Microsoft/Crypto/RSA/S-1-5-21-1/a-key", Credentials),
    ("AppData/Roaming/Bitwarden/data.json", Credentials),
    ("AppData/Roaming/Bitwarden CLI/data.json", Credentials),
    (
        "AppData/Local/Packages/8bitSolutionsLLC.bitwardendesktop_h4e712dmw3xyy/LocalCache/Roaming/Bitwarden/data.json",
        Credentials,
    ),
    ("AppData/Local/1Password/data/1password.sqlite", Credentials),
    (
        "AppData/Roaming/Thunderbird/Profiles/a.default-release/ImapMail/x",
        MessageStore,
    ),
    (
        "AppData/Local/Thunderbird/Profiles/a.default-release/cache2/x",
        MessageStore,
    ),
    ("AppData/Local/Microsoft/Outlook/someone@example.org.ost", MessageStore),
    ("AppData/Roaming/Microsoft/Outlook/Outlook.srs", MessageStore),
    ("AppData/Local/Microsoft/Olk/EBWebView/Default/x", MessageStore),
    ("Documents/Outlook Files/archive.pst", MessageStore),
    ("AppData/Local/Google/Chrome/User Data/Default/Login Data", BrowserData),
    (
        "AppData/Local/Google/Chrome Beta/User Data/Default/Cookies",
        BrowserData,
    ),
    ("AppData/Local/Google/Chrome SxS/User Data/Default/Cookies", BrowserData),
    ("AppData/Local/Chromium/User Data/Default/Cookies", BrowserData),
    (
        "AppData/Local/BraveSoftware/Brave-Browser/User Data/Default/Cookies",
        BrowserData,
    ),
    ("AppData/Local/Microsoft/Edge/User Data/Default/Login Data", BrowserData),
    (
        "AppData/Local/Microsoft/Edge Beta/User Data/Default/Cookies",
        BrowserData,
    ),
    (
        "AppData/Local/Microsoft/Edge SxS/User Data/Default/Cookies",
        BrowserData,
    ),
    ("AppData/Local/Vivaldi/User Data/Default/Cookies", BrowserData),
    ("AppData/Roaming/Opera Software/Opera Stable/Cookies", BrowserData),
    ("AppData/Local/Opera Software/Opera Stable/Cache/x", BrowserData),
    (
        "AppData/Roaming/Mozilla/Firefox/Profiles/a.default-release/logins.json",
        BrowserData,
    ),
    (
        "AppData/Local/Mozilla/Firefox/Profiles/a.default-release/cache2/x",
        BrowserData,
    ),
    (
        "AppData/Local/Packages/Mozilla.Firefox_n80bbvh6b1yt2/LocalCache/Roaming/Mozilla/Firefox/Profiles/a/logins.json",
        BrowserData,
    ),
    (
        "AppData/Local/Packages/TheBrowserCompany.Arc_ttt1ap7aakyb4/LocalCache/Local/Arc/User Data/Default/Cookies",
        BrowserData,
    ),
    // The compatibility junctions every profile carries.
    ("Application Data/Mozilla/Firefox/Profiles/a/logins.json", BrowserData),
    (
        "Local Settings/Application Data/Google/Chrome/User Data/Default/Cookies",
        BrowserData,
    ),
];

fn homes() -> [(&'static str, &'static [Cell]); 3] {
    [(MAC_HOME, MAC), (LINUX_HOME, LINUX), (WINDOWS_HOME, WINDOWS)]
}

/// No moved dirs: the cells must hold on spelling alone, whatever this machine's environment.
fn classify_plain(path: &str) -> Option<Protected> {
    classify_on(path, &[])
}

#[test]
fn every_os_s_locations_are_protected() {
    for (home, cells) in homes() {
        for (rest, reason) in cells {
            let path = format!("{home}/{rest}");
            assert_eq!(classify_plain(&path), Some(*reason), "{path}");
        }
    }
}

/// The folder a cell's file sits in, or the file itself when the cell names one at the top of
/// the home folder.
fn parent_of(rest: &str) -> &str {
    rest.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn name_of(rest: &str) -> &str {
    rest.rsplit_once('/').map_or(rest, |(_, name)| name)
}

/// Every way a tool can be handed the same file.
fn spellings(home: &str, rest: &str) -> Vec<String> {
    let path = format!("{home}/{rest}");
    let (parent, name) = (parent_of(rest), name_of(rest));
    let in_parent = |spelled: &str| match parent.is_empty() {
        true => format!("{home}/{spelled}"),
        false => format!("{home}/{parent}/{spelled}"),
    };
    vec![
        path.to_uppercase(),
        path.to_lowercase(),
        format!("{path}/"),
        format!("{path}//"),
        path.replace('/', "\\"),
        path.replace('/', "//"),
        path.replace('/', "/./"),
        // `..` out of a folder that isn't there, at the root, in the middle, and at the end.
        format!("{home}/elsewhere/../{rest}"),
        in_parent(&format!("gone/../{name}")),
        format!("{path}/sub/.."),
        // A backup of the home folder on a share, on a phone, and inside an archive.
        format!("/Volumes/testshare/backup{}", path.trim_start_matches("C:")),
        format!("smb://nas/share/{rest}"),
        format!("//server/share/profiles/someone/{rest}"),
        format!("mtp://device/Internal/{rest}"),
        format!("{home}/backup.zip/{rest}"),
        // An NFD name above the rule.
        format!("{home}/Jose\u{301}/{rest}"),
        // Win32 opens these as the same file: a trailing dot or space, and a named stream.
        format!("{path}."),
        format!("{path} "),
        format!("{path}::$DATA"),
        format!("{path}:stream"),
        match parent.is_empty() {
            true => path.clone(),
            false => format!("{home}/{}/{name}", parent.replace('/', "./")),
        },
    ]
}

#[test]
fn a_protected_location_is_refused_in_every_spelling() {
    for (home, cells) in homes() {
        for (rest, reason) in cells {
            for path in spellings(home, rest) {
                assert_eq!(classify_plain(&path), Some(*reason), "{path:?} (from {rest})");
            }
        }
    }
}

/// The table `sensitive_tests.rs` pins, in every spelling too: nothing that was protected
/// stops being protected.
#[test]
fn the_original_locations_hold_in_every_spelling() {
    let cells: &[Cell] = &[
        (".ssh/id_rsa", Credentials),
        (".gnupg/private-keys-v1.d/a.key", Credentials),
        (".password-store/bank.gpg", Credentials),
        (".aws/credentials", Credentials),
        (".azure/accessTokens.json", Credentials),
        (".kube/config", Credentials),
        (".config/gcloud/credentials.db", Credentials),
        (".config/gh/hosts.yml", Credentials),
        (".docker/config.json", Credentials),
        ("projects/app/.git/config", Credentials),
        (".cargo/credentials.toml", Credentials),
        (".claude/.credentials.json", Credentials),
        (".config/sops/age/keys.txt", Credentials),
        ("Library/Keychains/login.keychain-db", Credentials),
        (".local/share/keyrings/login.keyring", Credentials),
        (".netrc", Credentials),
        (".npmrc", Credentials),
        (".pypirc", Credentials),
        (".git-credentials", Credentials),
        ("projects/app/.env", Credentials),
        (".pgpass", Credentials),
        (".vault-token", Credentials),
        (".config/rclone/rclone.conf", Credentials),
        ("Library/Mail/V10/x.emlx", MessageStore),
        ("Library/Messages/chat.db", MessageStore),
        (".mozilla/firefox/a.default/logins.json", BrowserData),
        ("Library/Cookies/Cookies.binarycookies", BrowserData),
        ("Library/Safari/History.db", BrowserData),
        ("Library/Application Support/Arc/User Data/a", BrowserData),
        (".config/microsoft-edge/Default/a", BrowserData),
    ];
    for (rest, reason) in cells {
        for path in spellings(MAC_HOME, rest) {
            assert_eq!(classify_plain(&path), Some(*reason), "{path:?} (from {rest})");
        }
    }
}

/// A family rule takes a release channel's folder, never a neighbor that only shares a prefix,
/// and an app's name outside its anchor is an ordinary folder.
#[test]
fn neighbors_of_a_protected_location_stay_readable() {
    for path in [
        "C:/Users/someone/AppData/Local/Microsoft/EdgeUpdate/Log/a.log",
        "C:/Users/someone/AppData/Local/Google/ChromeFoo/a.txt",
        "C:/Users/someone/AppData/Local/Microsoft/Windows/Fonts/a.ttf",
        "C:/Users/someone/AppData/Roaming/Microsoft/Windows/Recent/a.lnk",
        "C:/Users/someone/AppData/Roaming/Code/User/settings.json",
        "C:/Users/someone/AppData/Local/Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
        "C:/Users/someone/Documents/Mozilla/a.txt",
        "C:/Users/someone/Documents/Outlook/a.txt",
        "C:/Users/someone/Documents/Thunderbird/a.txt",
        "C:/Users/someone/Documents/1Password/a.txt",
        "C:/Users/someone/Documents/Credentials/a.txt",
        "C:/Users/someone/Documents/Microsoft/Credentials/a.txt",
        "C:/Users/someone/Documents/AppData/a.txt",
        "D:/Data/Roaming/Mozilla/a.txt",
        "/home/someone/.config/operator/a.conf",
        "/home/someone/.config/fish/config.fish",
        "/home/someone/.config/Code/User/settings.json",
        "/home/someone/.config/evolution-data-server-like/a",
        "/home/someone/.cache/thumbnails/large/a.png",
        "/home/someone/.local/share/Trash/files/a.txt",
        "/home/someone/.local/share/applications/firefox.desktop",
        "/home/someone/.var/app/org.gimp.GIMP/config/GIMP/a",
        "/home/someone/snap/code/current/a",
        "/home/someone/Documents/thunderbird/a.txt",
        "/home/someone/Documents/evolution/a.txt",
        "/home/someone/Documents/mozilla/a.txt",
        "/home/someone/Documents/opera/a.txt",
        "/home/someone/Documents/vivaldi/a.txt",
        "/home/someone/Documents/geary/a.txt",
        "/home/someone/Documents/akonadi/a.txt",
        "/home/someone/Documents/keyrings/a.txt",
        "/home/someone/Documents/gh/a.txt",
        "/Users/someone/Library/Application Support/Google Earth/a.txt",
        "/Users/someone/Library/Application Support/Google/DriveFS/a.txt",
        "/Users/someone/Library/Application Support/Arcade/a.txt",
        "/Users/someone/Library/Application Support/Vivaldium/a.txt",
        "/Users/someone/Library/Containers/com.apple.TextEdit/Data/a.txt",
        "/Users/someone/Library/Containers/com.apple.mailbox/Data/a.txt",
        "/Users/someone/Library/Caches/com.apple.Safarix/a",
        "/Users/someone/Library/Group Containers/UBF8T346G9.Office/Word/a.txt",
        "/Users/someone/Documents/Thunderbird/a.txt",
        "/Users/someone/Documents/Bitwarden/a.txt",
        "/Users/someone/Documents/Outlook Files Backup/a.txt",
        "/Users/someone/pgpass.conf.txt",
    ] {
        assert_eq!(classify_plain(path), None, "{path}");
    }
}

fn moved(dirs: &[(Dir, &str)]) -> Vec<(Dir, PathBuf)> {
    dirs.iter().map(|(dir, path)| (*dir, PathBuf::from(path))).collect()
}

/// A per-user dir the environment moved has no telling component, so only the host's own
/// answer finds what's in it.
#[test]
fn a_moved_per_user_dir_is_protected_where_the_host_keeps_it() {
    let host = folded_dirs(&moved(&[
        (Dir::XdgConfig, "/data/cfg"),
        (Dir::XdgData, "/data/share"),
        (Dir::WinRoaming, "//files/redirect/someone/Roam"),
        (Dir::WinLocal, "D:/Profiles/someone/Local"),
    ]));
    let cells: &[Cell] = &[
        ("/data/cfg/gh/hosts.yml", Credentials),
        ("/data/cfg/google-chrome-beta/Default/Cookies", BrowserData),
        ("/data/cfg/mozilla/firefox/a.default/logins.json", BrowserData),
        ("/data/share/keyrings/login.keyring", Credentials),
        ("/data/share/evolution/mail/local/cur/1", MessageStore),
        (
            "//files/redirect/someone/Roam/Mozilla/Firefox/Profiles/a/logins.json",
            BrowserData,
        ),
        ("//files/redirect/someone/Roam/Microsoft/Credentials/ABCD", Credentials),
        ("//files/redirect/someone/Roam/Thunderbird/Profiles/a/x", MessageStore),
        (
            "D:/Profiles/someone/Local/Google/Chrome/User Data/Default/Cookies",
            BrowserData,
        ),
        ("D:/Profiles/someone/Local/Microsoft/Outlook/a.ost", MessageStore),
        // The same files in other spellings.
        ("/DATA/CFG/GH/hosts.yml", Credentials),
        ("/data/cfg/./gh/x/../hosts.yml", Credentials),
        ("\\\\FILES\\redirect\\someone\\roam\\mozilla\\firefox\\a", BrowserData),
        ("d:\\profiles\\someone\\local\\google\\chrome sxs\\a", BrowserData),
    ];
    for (path, reason) in cells {
        assert_eq!(classify_on(path, &host), Some(*reason), "{path}");
        assert_eq!(classify_on(path, &[]), None, "{path} has no telling component");
    }
    for path in [
        "/data/cfg/fish/config.fish",
        "/data/share/applications/a.desktop",
        "/data/cfg",
        // A rule belongs to its own dir: `keyrings` is a data-dir name, not a config-dir one.
        "/data/cfg/keyrings/a",
        "/other/data/cfg/gh/hosts.yml",
        "//files/redirect/someone/Roam/Code/settings.json",
        "D:/Profiles/someone/Local/Temp/a.txt",
    ] {
        assert_eq!(classify_on(path, &host), None, "{path}");
    }
}

/// The host's dir may itself be a link, and so may the path asked for.
#[test]
fn the_gate_follows_links_into_and_out_of_a_moved_dir() {
    let dir = TestDir::new("gate-moved-dir");
    let real_config = dir.join("real-config");
    std::fs::create_dir_all(real_config.join("gh")).unwrap();
    let token = real_config.join("gh/hosts.yml");
    std::fs::write(&token, "token").unwrap();
    let config_link = dir.join("config-link");
    std::os::unix::fs::symlink(&real_config, &config_link).unwrap();
    let file_link = dir.join("innocent.txt");
    std::os::unix::fs::symlink(&token, &file_link).unwrap();
    let plain = dir.join("plain.txt");
    std::fs::write(&plain, "hello").unwrap();

    for host_dir in [&real_config, &config_link] {
        let host = vec![(Dir::XdgConfig, host_dir.clone())];
        for path in [&token, &config_link.join("gh/hosts.yml"), &file_link] {
            assert_eq!(
                gate_on(path.to_str().unwrap(), None, &host),
                Err(Credentials),
                "{path:?} with the host dir at {host_dir:?}"
            );
        }
        assert_eq!(gate_on(plain.to_str().unwrap(), None, &host), Ok(()));
    }
    assert_eq!(gate_on(token.to_str().unwrap(), None, &[]), Ok(()));
}

#[test]
fn a_symlink_into_a_mail_or_browser_folder_is_refused_by_the_gate() {
    let dir = TestDir::new("gate-symlink-mail");
    let cells: &[Cell] = &[
        (".thunderbird/a.default/Mail/Inbox", MessageStore),
        (".config/google-chrome/Default/Cookies", BrowserData),
        ("AppData/Roaming/Microsoft/Credentials/ABCD", Credentials),
    ];
    for (index, (rest, reason)) in cells.iter().enumerate() {
        let file = dir.join(rest);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "x").unwrap();
        let link = dir.join(format!("innocent-{index}.txt"));
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let link = link.to_str().unwrap();
        assert_eq!(classify_plain(link), None, "the name alone gives nothing away");
        assert_eq!(gate_on(link, None, &[]), Err(*reason), "{rest}");
    }
}
