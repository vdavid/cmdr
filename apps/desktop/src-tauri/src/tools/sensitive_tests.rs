//! The gate's rules, table by table, plus the symlink and data-dir checks only `path_gate` makes.

use std::time::Duration;

use super::*;
use crate::test_support::TestDir;

const HOME: &str = "/Users/someone";

fn home(rest: &str) -> String {
    format!("{HOME}/{rest}")
}

#[test]
fn every_listed_folder_and_file_is_protected() {
    let cells: &[(&str, Protected)] = &[
        (".ssh/id_rsa", Protected::Credentials),
        (".ssh", Protected::Credentials),
        (".gnupg/private-keys-v1.d/a.key", Protected::Credentials),
        (".password-store/bank.gpg", Protected::Credentials),
        (".aws/credentials", Protected::Credentials),
        (".azure/accessTokens.json", Protected::Credentials),
        (".kube/config", Protected::Credentials),
        (".config/gcloud/credentials.db", Protected::Credentials),
        (".config/gh/hosts.yml", Protected::Credentials),
        (".docker/config.json", Protected::Credentials),
        ("projects/app/.git/config", Protected::Credentials),
        (".cargo/credentials.toml", Protected::Credentials),
        (".claude/.credentials.json", Protected::Credentials),
        (".config/sops/age/keys.txt", Protected::Credentials),
        ("Library/Keychains/login.keychain-db", Protected::Credentials),
        (".local/share/keyrings/login.keyring", Protected::Credentials),
        (".netrc", Protected::Credentials),
        (".npmrc", Protected::Credentials),
        (".pypirc", Protected::Credentials),
        (".git-credentials", Protected::Credentials),
        ("projects/app/.env", Protected::Credentials),
        ("projects/app/.env.local", Protected::Credentials),
        (".pgpass", Protected::Credentials),
        (".vault-token", Protected::Credentials),
        (".config/rclone/rclone.conf", Protected::Credentials),
        ("Library/Mail/V10/x.emlx", Protected::MessageStore),
        ("Library/Messages/chat.db", Protected::MessageStore),
        (
            "Library/Group Containers/group.com.apple.notes/NoteStore.sqlite",
            Protected::MessageStore,
        ),
        (".mozilla/firefox/a.default/logins.json", Protected::BrowserData),
        ("Library/Cookies/Cookies.binarycookies", Protected::BrowserData),
        ("Library/Safari/History.db", Protected::BrowserData),
        (
            "Library/Application Support/Google/Chrome/Default/Login Data",
            Protected::BrowserData,
        ),
        ("Library/Application Support/Chromium/Default/a", Protected::BrowserData),
        (
            "Library/Application Support/BraveSoftware/Brave-Browser/a",
            Protected::BrowserData,
        ),
        (
            "Library/Application Support/Microsoft Edge/Default/a",
            Protected::BrowserData,
        ),
        ("Library/Application Support/Arc/User Data/a", Protected::BrowserData),
        ("Library/Application Support/Firefox/Profiles/a", Protected::BrowserData),
        (".config/google-chrome/Default/a", Protected::BrowserData),
        (".config/chromium/Default/a", Protected::BrowserData),
        (".config/BraveSoftware/Brave-Browser/a", Protected::BrowserData),
        (".config/microsoft-edge/Default/a", Protected::BrowserData),
        (
            "Library/Application Support/com.veszelovszki.cmdr/mcp.token",
            Protected::CmdrData,
        ),
        (
            "Library/Application Support/com.veszelovszki.cmdr-dev-slug/mcp.token",
            Protected::CmdrData,
        ),
    ];
    for (rest, reason) in cells {
        assert_eq!(classify(&home(rest)), Some(*reason), "{rest}");
    }
}

#[test]
fn a_rule_holds_at_any_depth_case_and_unicode_form() {
    for path in [
        "/Volumes/naspi/backup/home/.ssh/id_rsa",
        "/Users/someone/.SSH/id_rsa",
        "/Users/someone/.Ssh/ID_RSA",
        // An NFD name above the rule mustn't hide it.
        "/Users/Jose\u{301}/.ssh/id_rsa",
        // Inside an archive: components are the whole check before anything materializes.
        "/Users/someone/foo.zip/.ssh/id_rsa",
        "/Users/someone/foo.zip/home\\.ssh\\id_rsa",
        // No realpath for these: components are all there is.
        "mtp://device/Internal/.ssh/id_rsa",
        "smb://nas/share/home/.ssh/id_rsa",
        // `..` resolved lexically, for a path no realpath can reach.
        "mtp://device/Internal/x/../.ssh/id_rsa",
    ] {
        assert_eq!(classify(path), Some(Protected::Credentials), "{path}");
    }
    assert_eq!(
        classify("/Users/someone/.\u{212A}ube/config"),
        Some(Protected::Credentials)
    );
    assert_eq!(
        classify("/Users/Jose\u{301}/Library/MAIL/a.emlx"),
        Some(Protected::MessageStore)
    );
}

/// Case-folded at any depth, a bare `notes` or `chrome` would refuse ordinary folders.
#[test]
fn ordinary_paths_with_alarming_names_stay_readable() {
    for path in [
        "/Users/someone/Documents/notes/a.txt",
        "/Users/someone/Documents/Notes/a.txt",
        "/Users/someone/Documents/Chrome/a.txt",
        "/Users/someone/Documents/Edge/a.txt",
        "/Users/someone/Documents/Arc/a.txt",
        "/Users/someone/Documents/Brave/a.txt",
        "/Users/someone/Documents/Safari/a.txt",
        "/Users/someone/Documents/Firefox/a.txt",
        "/Users/someone/Library/Application Support/Notes/a.txt",
        "/Users/someone/Documents/config",
        "/Users/someone/project/config.json",
        "/Users/someone/project/.git/HEAD",
        "/Users/someone/.config/fish/config.fish",
        "/Users/someone/mail/inbox.txt",
        // A `.env` folder is a Python venv: only the file name protects.
        "/Users/someone/project/.env/lib/site.py",
        "/Users/someone/mysshkeys.txt",
    ] {
        assert_eq!(classify(path), None, "{path}");
    }
}

#[test]
fn a_symlink_into_a_protected_folder_is_refused_by_the_gate() {
    let dir = TestDir::new("gate-symlink");
    let ssh = dir.join(".ssh");
    std::fs::create_dir(&ssh).unwrap();
    std::fs::write(ssh.join("id_rsa"), "key").unwrap();
    let file_link = dir.join("innocent.txt");
    std::os::unix::fs::symlink(ssh.join("id_rsa"), &file_link).unwrap();
    let folder_link = dir.join("innocent");
    std::os::unix::fs::symlink(&ssh, &folder_link).unwrap();

    let file_link = file_link.to_str().unwrap();
    assert_eq!(classify(file_link), None, "the name alone gives nothing away");
    assert_eq!(path_gate(file_link), Err(Protected::Credentials));
    let through_folder = folder_link.join("id_rsa");
    assert_eq!(path_gate(through_folder.to_str().unwrap()), Err(Protected::Credentials));
    // A path that doesn't exist yet still resolves through the link above it.
    let missing = folder_link.join("gone");
    assert_eq!(path_gate(missing.to_str().unwrap()), Err(Protected::Credentials));
    // So does an archive path, whose inner part is no file on disk.
    let inner = folder_link.join("backup.zip/inner.txt");
    assert_eq!(path_gate(inner.to_str().unwrap()), Err(Protected::Credentials));
}

#[test]
fn a_plain_file_passes_the_gate() {
    let dir = TestDir::new("gate-plain");
    let file = dir.join("a.txt");
    std::fs::write(&file, "hello").unwrap();
    assert_eq!(path_gate(file.to_str().unwrap()), Ok(()));
    assert_eq!(path_gate_within(file.to_str().unwrap(), Duration::from_secs(5)), Ok(()));
}

/// A custom `CMDR_DATA_DIR` has no telling component, so only the check against the real dir
/// catches it, through a symlink too.
#[test]
fn a_custom_data_dir_is_caught_by_the_gate_alone() {
    let dir = TestDir::new("gate-data-dir");
    let data = dir.join("elsewhere");
    std::fs::create_dir(&data).unwrap();
    let token = data.join("mcp.token");
    std::fs::write(&token, "secret").unwrap();
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink(&token, &link).unwrap();

    assert_eq!(classify(token.to_str().unwrap()), None);
    assert_eq!(
        gate_against(token.to_str().unwrap(), Some(&data)),
        Err(Protected::CmdrData)
    );
    assert_eq!(
        gate_against(link.to_str().unwrap(), Some(&data)),
        Err(Protected::CmdrData)
    );
    assert_eq!(
        gate_against(dir.join("other.txt").to_str().unwrap(), Some(&data)),
        Ok(())
    );
}

/// The gate reads the same dir the app does, `CMDR_DATA_DIR` included. Under test that's the
/// process's scratch dir, which has no telling component either.
#[test]
fn the_gate_reads_cmdr_s_own_resolved_data_dir() {
    let data = crate::config::standalone_app_data_dir().expect("a test has a data dir");
    let token = data.join("mcp.token");
    assert_eq!(classify(token.to_str().unwrap()), None);
    assert_eq!(path_gate(token.to_str().unwrap()), Err(Protected::CmdrData));
}

#[test]
fn the_bounded_gate_answers_a_protected_path_without_a_filesystem_call() {
    assert_eq!(
        path_gate_within("/Users/someone/.ssh/id_rsa", Duration::ZERO),
        Err(GateStop::Protected(Protected::Credentials))
    );
}

#[test]
fn private_key_armor_is_refused_at_either_end() {
    for text in [
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN ENCRYPTED PRIVATE KEY-----",
        "-----BEGIN PGP PRIVATE KEY BLOCK-----",
        "-----END OPENSSH PRIVATE KEY-----",
        "b3BlbnNzaC1rZXktdjEAAAA\n-----END RSA PRIVATE KEY-----\ntrailing",
        r#"{"private_key": "-----BEGIN PRIVATE KEY-----\nMIIE"}"#,
    ] {
        assert_eq!(refuse_private_key_text(text), Err(Protected::PrivateKey), "{text}");
    }
}

#[test]
fn prose_about_keys_and_public_keys_pass() {
    for text in [
        "",
        "Never share your private key.",
        "-----BEGIN PUBLIC KEY-----",
        "-----BEGIN CERTIFICATE-----",
        "ssh-ed25519 AAAAC3Nza someone@example",
    ] {
        assert_eq!(refuse_private_key_text(text), Ok(()), "{text}");
    }
}
