//! The device shell the fake server answers with: enough `toybox` to run the
//! verbs `writes.rs` reaches for, and exit code 127 for anything else.
//!
//! ❗ It answers with EXIT CODES, never wording the crate parses: the real
//! backend classifies a failure by re-probing the path, so this fixture must
//! never become the reason a stderr string looks load-bearing.

use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::errors::{EEXIST, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY_DEVICE, EROFS};

use super::tree::{FakeMount, FakeNode, FakeTree};

/// Splits a POSIX command line into words: single quotes literal, double quotes
/// and backslashes handled minimally.
pub fn split_argv(line: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    cur.push(q);
                }
            }
            '"' => {
                in_word = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => {
                            if let Some(n) = chars.next() {
                                cur.push(n);
                            }
                        }
                        other => cur.push(other),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    argv.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            other => {
                in_word = true;
                cur.push(other);
            }
        }
    }
    if in_word {
        argv.push(cur);
    }
    argv
}

fn errno_text(errno: i32) -> &'static str {
    match errno {
        ENOENT => "No such file or directory",
        EEXIST => "File exists",
        EISDIR => "Is a directory",
        EROFS => "Read-only file system",
        ENOTEMPTY_DEVICE => "Directory not empty",
        ENOTDIR => "Not a directory",
        _ => "Unknown error",
    }
}

/// `df -k`'s table as toybox lays it out: every column as wide as its widest
/// cell, header included, and the `Filesystem` column at least 14 wide. The
/// header prints even with no rows. Held byte for byte against a real phone by
/// `shell_test.rs` and [`pixel_captures`](super::pixel_captures).
fn df_k_table(mounts: &[&FakeMount]) -> String {
    let header = ["Filesystem", "1K-blocks", "Used", "Available", "Use%", "Mounted on"].map(String::from);
    let rows: Vec<[String; 6]> = mounts
        .iter()
        .map(|m| {
            [
                m.device.clone(),
                m.total_kib.to_string(),
                m.used_kib.to_string(),
                m.available_kib.to_string(),
                format!("{}%", m.use_percent()),
                m.mounted_on.clone(),
            ]
        })
        .collect();
    let mut widths = [14, 0, 0, 0, 0, 0];
    for row in std::iter::once(&header).chain(&rows) {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }
    let [w_device, w_total, w_used, w_available, w_percent, _] = widths;
    std::iter::once(&header)
        .chain(&rows)
        .map(|[device, total, used, available, percent, mounted_on]| {
            format!(
                "{device:<w_device$} {total:>w_total$} {used:>w_used$} {available:>w_available$} {percent:>w_percent$} {mounted_on}\n"
            )
        })
        .collect()
}

/// Runs one fake shell command over the tree: `(exit_code, stdout, stderr)`.
pub fn run_fake_shell(tree: &Mutex<FakeTree>, argv: &[String]) -> (u8, String, String) {
    let Some(cmd) = argv.first() else {
        return (0, String::new(), String::new());
    };
    let flags: Vec<&str> = argv[1..]
        .iter()
        .map(String::as_str)
        .filter(|a| a.starts_with('-'))
        .collect();
    let args: Vec<&str> = argv[1..]
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let mut tree = tree.lock_ignore_poison();
    match cmd.as_str() {
        "mkdir" => {
            for p in &args {
                let result = if flags.contains(&"-p") {
                    tree.mkdir_p(p)
                } else if tree.get(p).is_some() {
                    Err(EEXIST)
                } else {
                    tree.mkdir_p(p)
                };
                if let Err(e) = result {
                    return (1, String::new(), format!("mkdir: '{p}': {}\n", errno_text(e)));
                }
            }
            (0, String::new(), String::new())
        }
        "rm" => {
            let recursive = flags.iter().any(|f| f.contains('r'));
            let force = flags.iter().any(|f| f.contains('f'));
            for p in &args {
                let result = if recursive {
                    tree.remove_tree(p)
                } else {
                    tree.remove_one(p)
                };
                match result {
                    Ok(()) => {}
                    Err(ENOENT) if force => {}
                    Err(e) => return (1, String::new(), format!("rm: {p}: {}\n", errno_text(e))),
                }
            }
            (0, String::new(), String::new())
        }
        "mv" => {
            let [from, to] = args[..] else {
                return (1, String::new(), "mv: need two arguments\n".to_string());
            };
            match tree.rename(from, to) {
                Ok(()) => (0, String::new(), String::new()),
                Err(e) => (
                    1,
                    String::new(),
                    format!("mv: bad rename of '{from}': {}\n", errno_text(e)),
                ),
            }
        }
        "df" => {
            if args.is_empty() {
                let every: Vec<&FakeMount> = tree.mounts().iter().collect();
                return (0, df_k_table(&every), String::new());
            }
            // Like toybox: a missing path's reason goes to stderr and makes the
            // exit 1, while the paths that were found keep their rows.
            let mut found = Vec::new();
            let mut reasons = Vec::new();
            for p in &args {
                match tree.mount_for(p) {
                    Ok(mount) => found.push(mount),
                    Err(e) => reasons.push(format!("df: '{p}': {}\n", errno_text(e))),
                }
            }
            (u8::from(!reasons.is_empty()), df_k_table(&found), reasons.concat())
        }
        "readlink" => {
            let Some(p) = args.first() else {
                return (1, String::new(), String::new());
            };
            match tree.resolve(p) {
                Ok(target) => (0, format!("{target}\n"), String::new()),
                Err(_) => (1, String::new(), String::new()),
            }
        }
        "test" => {
            let node = args.first().and_then(|p| tree.get(p));
            let ok = match flags.first().copied().unwrap_or("-e") {
                "-e" => node.is_some(),
                "-d" => matches!(node, Some(FakeNode::Dir { .. })),
                "-f" => matches!(node, Some(FakeNode::File { .. })),
                "-w" => node.is_some() && !tree.read_only,
                _ => false,
            };
            (u8::from(!ok), String::new(), String::new())
        }
        "rmdir" => {
            for p in &args {
                let result = match tree.get(p) {
                    Some(FakeNode::Dir { .. }) => tree.remove_one(p),
                    Some(_) => Err(ENOTDIR),
                    None => Err(ENOENT),
                };
                if let Err(e) = result {
                    return (1, String::new(), format!("rmdir: '{p}': {}\n", errno_text(e)));
                }
            }
            (0, String::new(), String::new())
        }
        "cp" => {
            let [from, to] = args[..] else {
                return (1, String::new(), "cp: need two arguments\n".to_string());
            };
            let result = match tree.get(from).cloned() {
                Some(FakeNode::File { data, mode, mtime }) => tree.write_file(to, data, mode, mtime),
                Some(_) => Err(EISDIR),
                None => Err(ENOENT),
            };
            match result {
                Ok(()) => (0, String::new(), String::new()),
                Err(e) => (
                    1,
                    String::new(),
                    format!("cp: bad copy of '{from}': {}\n", errno_text(e)),
                ),
            }
        }
        "stat" => {
            let Some(p) = args.last() else {
                return (1, String::new(), String::new());
            };
            let format = args.first().filter(|_| args.len() > 1).copied().unwrap_or("%n");
            match tree.stat(p) {
                Ok(node) => {
                    let line = format
                        .replace("%f", &format!("{:x}", node.mode()))
                        .replace("%s", &node.size().to_string())
                        .replace("%Y", &node.mtime().to_string())
                        .replace("%n", p);
                    (0, format!("{line}\n"), String::new())
                }
                Err(e) => (1, String::new(), format!("stat: '{p}': {}\n", errno_text(e))),
            }
        }
        other => (
            127,
            String::new(),
            format!("/system/bin/sh: {other}: inaccessible or not found\n"),
        ),
    }
}
