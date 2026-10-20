//! The producers' real output through the span pass: `LogPrivate` at a `{}` and a `{:?}` site,
//! `LogDetail::marking`, and the hostile names each must survive. Every cell feeds what
//! `cmdr_fs` writes, never a hand-built line, and checks it hashes like the hand-built spellings
//! `markers_tests.rs` pins.

use super::markers_tests::{HEAD, assert_ships_none, context, hashes, quoted, r, shape, span};
use cmdr_fs::log_detail::{LOCAL_DETAIL_MAX_BYTES, LogDetail, LoggedText};
use cmdr_fs::log_private::{CLOSER, LogKind, LogPrivate, RESERVED};

const FILE: &str = "/Users/me/Pictures/Summer (Lake House) 2026/IMG 0001.jpg";

/// Names that try every way out of a span. Each holds `Lake` so a leak is easy to spot.
fn hostile_values() -> Vec<String> {
    let mut values: Vec<String> = RESERVED.map(|c| format!("Lake{c}House")).collect();
    values.extend(
        [
            "Summer (Lake House) 2026",
            "Lake\\u{fdd1}House",
            "Lake\\\\u{fdd1}House",
            "Lake \"House\"",
            "Lake House\\",
            "Lake\nHouse\r\t",
            "Lake\u{202E}fdp.exe",
            "Lake Cafe\u{301}",
            "<dir:abc>Lake",
            "Lake: House, a; b key=v -> )",
        ]
        .map(str::to_string),
    );
    values
}

#[test]
fn a_wrapped_value_hashes_like_the_hand_built_span_at_both_sites() {
    for kind in LogKind::ALL {
        for value in hostile_values() {
            let wrapped = LogPrivate::new(kind, value.as_str());
            let written = format!("{HEAD}moved {wrapped} item={wrapped:?} in 3ms");
            let by_hand = format!(
                "{HEAD}moved {} item={} in 3ms",
                span(kind, &value),
                quoted(kind, &value)
            );
            assert_eq!(written, by_hand, "{kind:?} {value:?}");
            assert_eq!(
                context().redact_line(&written),
                context().redact_line(&by_hand),
                "{kind:?} {value:?}"
            );
            let words = ["Lake", "House", "Summer", "fdp", "Cafe"];
            assert_ships_none(&written, &words);
            let tokens = hashes(&context().redact_line(&written));
            let (first, second) = tokens.split_at(tokens.len() / 2);
            assert!(!first.is_empty() && first == second, "{kind:?} {value:?}: {tokens:?}");
        }
    }
}

#[test]
fn a_wrapped_path_keeps_its_shape_and_kind() {
    let line = format!(
        "{HEAD}list_directory_core: 0 entries at {} (exists, readable). Could be genuinely empty",
        LogPrivate::dir_path("/Users/me/Pictures/com.example.app")
    );
    assert_eq!(
        r(&line),
        format!(
            "{HEAD}list_directory_core: 0 entries at $HOME/Pictures/<dir> (exists, readable). Could be genuinely empty"
        )
    );
    let line = format!("{HEAD}read path={:?}", LogPrivate::file_path(FILE));
    assert_eq!(
        shape(&line),
        format!("{HEAD}read path=\"$HOME/Pictures/<dir>/<file>.jpg\"")
    );
}

/// External text that echoes the caller's value carries it one depth down; the leaf and the echo
/// on one line hash to the one token.
#[test]
fn a_marked_detail_hashes_like_the_leaf_on_its_line() {
    let stderr = format!("open({FILE}): Permission denied");
    let line = format!(
        "{HEAD}copy failed path={:?} detail={:?}",
        LogPrivate::file_path(FILE),
        LogDetail(&stderr).marking(&[LogPrivate::file_path(FILE)])
    );
    assert_ships_none(&line, &["Lake", "House", "IMG", "0001"]);
    let out = context().redact_line(&line).into_owned();
    let tokens = hashes(&out);
    assert!(tokens.len() >= 2, "{out}");
    let file_tokens: Vec<_> = out.match_indices("<file:").map(|(at, _)| &out[at..at + 19]).collect();
    assert_eq!(file_tokens.len(), 2, "{out}");
    assert_eq!(file_tokens[0], file_tokens[1], "{out}");
}

#[test]
fn the_detail_cap_never_ships_a_name_at_any_offset() {
    for lead in LOCAL_DETAIL_MAX_BYTES - FILE.len() - 4..=LOCAL_DETAIL_MAX_BYTES + 4 {
        let stderr = format!("{} {FILE} tail", "-".repeat(lead));
        let line = format!(
            "{HEAD}copy failed detail={:?}",
            LogDetail(&stderr).marking(&[LogPrivate::file_path(FILE)])
        );
        assert_ships_none(&line, &["Lake", "House", "IMG", "0001"]);
    }
}

/// Plain `LogDetail` replaces a raw marker in foreign text, so an OS message echoing a hostile
/// name can't forge a closer at the depth real `LogDetail` spans sit at. The fallback regex still
/// claims the path whole.
#[test]
fn a_hostile_name_echoed_in_plain_external_text_is_claimed_whole() {
    let stderr = format!("open /Volumes/Backup/Secret Dir/evil{CLOSER}rest/child.pdf: EACCES");
    let line = format!("{HEAD}copy failed detail={:?}", LogDetail(&stderr));
    assert_ships_none(&line, &["Secret", "evil", "rest", "child"]);
}

/// A real log site, as a report ships it: `list_directory_core`'s two lines for an empty folder
/// whose name holds a paren, the shape the greedy fallback can only guess at. The folder goes as
/// one token and every word after it stays. The numbers vary per machine, so they're patterns.
#[test]
fn the_listing_lines_ship_their_dir_as_one_token_and_keep_the_rest() {
    let root = crate::test_support::TestDir::new("listing-golden");
    let dir = root.join("Summer (Lake House) 2026");
    std::fs::create_dir(&dir).expect("create the empty folder");
    let host = cmdr_fs::volume::host::VolumeHost::detached();
    let (listed, logs) =
        crate::test_support::capture_logs(|| cmdr_local::reading::list_directory_core(&dir, host.listings()));
    assert!(listed.expect("list the empty folder").is_empty());

    let token = regex::escape(&r(&LogPrivate::dir_path(&dir).to_string()));
    let goldens = [
        format!(r"^list_directory_core: path={token}, entries=0, read_dir=\d+ms, total=\d+ms$"),
        format!(
            r"^list_directory_core: 0 entries at {token} \(kind=dir, perms=0o\d+, size=\d+\)\. Could be genuinely empty, permission/TCC restricted \(Full Disk Access\?\), or a cloud-sync placeholder\.$"
        ),
    ];
    let lines: Vec<_> = logs
        .iter()
        .filter(|log| log.message.starts_with("list_directory_core:"))
        .map(|log| log.file_line())
        .collect();
    assert_eq!(lines.len(), goldens.len(), "{lines:#?}");
    for (line, golden) in lines.iter().zip(goldens) {
        assert_ships_none(line, &["Lake", "House", "Summer"]);
        let shipped = r(line);
        let message = &shipped[super::markers::message_start(&shipped)..];
        assert!(
            regex::Regex::new(&golden).expect("valid golden").is_match(message),
            "{message}\n{golden}"
        );
    }
}

/// A `log_error!` message that carries a marked `detail=` field, quoted again as the Flow B note's
/// own `detail=`: the inner field's span sits two depths down inside a field inside a field, and
/// still ships as the token the log line gives it, never as escaped placeholder text.
#[test]
fn a_span_in_a_detail_inside_a_detail_ships_its_token() {
    let stderr = format!("open({FILE}): Permission denied");
    let message = format!(
        "couldn't copy: detail={:?}; tail",
        LogDetail(&stderr).marking(&[LogPrivate::file_path(FILE)]),
    );
    let note = format!("auto-send: first: copy detail={:?}", LoggedText(&message));
    assert_ships_none(&note, &["Lake", "House", "IMG", "0001"]);
    let line = format!("{HEAD}{message}");
    let (in_note, on_line) = (context().redact_line(&note), context().redact_line(&line));
    assert_eq!(hashes(&in_note), hashes(&on_line), "{in_note}\n{on_line}");
    assert_eq!(hashes(&in_note).len(), 2, "{in_note}");
}

/// The index's own lines, in each site's format string, as a report ships them. Each pins what
/// the marker buys over the line's unmarked spelling: the kind the site knows, the stat after a
/// prose path, and a bare name no regex could find.
#[test]
fn the_index_lines_ship_each_value_as_one_token_of_its_kind() {
    // A dir whose name reads like a file: only the site knows it's a folder.
    let dir = "/Users/me/Library/Containers/com.example.app";
    let reconcile = |path: &dyn std::fmt::Debug| {
        format!("{HEAD}MustScanSubDirs: reconcile complete for path={path:?} (+0 -0 ~0, 6ms, 1ms CPU)")
    };
    let golden = format!(
        "{HEAD}MustScanSubDirs: reconcile complete for path=\"$HOME/Library/<dir>/<dir>\" (+0 -0 ~0, 6ms, 1ms CPU)"
    );
    assert_eq!(shape(&reconcile(&LogPrivate::dir_path(dir))), golden);
    assert_ne!(
        shape(&reconcile(&dir)),
        golden,
        "the unmarked spelling guesses the kind"
    );

    // A prose path followed by a parenthesized word, which the greedy fallback can't tell apart.
    let root = "/Users/me/Pictures/Summer (Lake House) 2026";
    let covering = |path: &dyn std::fmt::Display| format!("{HEAD}Phases: covering {path} (Home)");
    let marked = covering(&LogPrivate::dir_path(root));
    assert_ships_none(&marked, &["Lake", "House", "Summer"]);
    let golden = format!("{HEAD}Phases: covering $HOME/Pictures/<dir> (Home)");
    assert_eq!(shape(&marked), golden);
    assert_ne!(shape(&covering(&root)), golden, "the unmarked spelling eats the phase");

    // A bare entry name, which has no path around it for a regex to claim.
    let name = "IMG Lake 0001.jpg";
    let inserted =
        |name: &dyn std::fmt::Display| format!("{HEAD}Writer: UpsertEntryV2 inserted \"{name}\" (parent_id=5) → id=9");
    let marked = inserted(&LogPrivate::new(LogKind::FileName, name));
    assert_ships_none(&marked, &["Lake", "IMG", "0001"]);
    assert_eq!(
        shape(&marked),
        format!("{HEAD}Writer: UpsertEntryV2 inserted \"<file>.jpg\" (parent_id=5) → id=9")
    );
    assert!(
        shape(&inserted(&name)).contains("Lake"),
        "the unmarked name ships whole"
    );
}

/// The desktop crate's volume and SMB lines, in each site's format string, as a report ships
/// them. A prose mount path with parens or a space no longer eats the words after it, and a
/// share marked as a volume hashes like the keyed `share=` value it replaced.
#[test]
fn the_desktop_volume_and_smb_lines_ship_each_value_as_one_token_of_its_kind() {
    // A prose mount path whose name holds parens: the greedy fallback runs to the next `'`.
    let volume = "/Volumes/Summer (Lake House) 2026";
    let stranded = |path: &dyn std::fmt::Display| {
        format!("{HEAD}{path} unmounted and volume vol-abc can't move to one of its other mounts, so it stays there.")
    };
    let marked = stranded(&LogPrivate::dir_path(volume));
    assert_ships_none(&marked, &["Lake", "House", "Summer"]);
    let golden = format!(
        "{HEAD}/Volumes/<volume> unmounted and volume vol-abc can't move to one of its other mounts, so it stays there."
    );
    assert_eq!(shape(&marked), golden);
    assert_ne!(
        shape(&stranded(&volume)),
        golden,
        "the unmarked spelling eats the sentence"
    );

    // Two prose mount paths with a space in the share name.
    let taken = |default: &dyn std::fmt::Display, candidate: &dyn std::fmt::Display| {
        format!("{HEAD}Mount path {default} taken by another server, using {candidate}")
    };
    let marked = taken(
        &LogPrivate::dir_path("/Volumes/Family Photos"),
        &LogPrivate::dir_path("/Volumes/Family Photos-1"),
    );
    assert_ships_none(&marked, &["Family", "Photos"]);
    let golden = format!("{HEAD}Mount path /Volumes/<volume> taken by another server, using /Volumes/<volume>");
    assert_eq!(shape(&marked), golden);
    assert_ne!(
        shape(&taken(&"/Volumes/Family Photos", &"/Volumes/Family Photos-1")),
        golden,
        "the unmarked spelling eats the prose between the paths"
    );

    // Keyed identities: each marked value hashes like the keyed value it replaced.
    let refused = |who: &dyn std::fmt::Display, server: &dyn std::fmt::Debug, share: &dyn std::fmt::Debug| {
        format!(
            "{HEAD}server={server:?}, share={share:?} turned {who} away at sign-in (bad password); asking for a sign-in."
        )
    };
    let marked = refused(
        &format_args!("user={:?}", LogPrivate::user("alex")),
        &LogPrivate::host("Alex's MacBook Pro (2)"),
        &LogPrivate::volume("Family Photos"),
    );
    assert_ships_none(&marked, &["Alex", "MacBook", "Family", "Photos", "alex"]);
    assert_eq!(
        shape(&marked),
        format!(
            "{HEAD}server=\"<host>\", share=\"<volume>\" turned user=\"<user>\" away at sign-in (bad password); asking for a sign-in."
        )
    );
    let keyed = refused(&"user=\"alex\"", &"Alex's MacBook Pro (2)", &"Family Photos");
    assert_eq!(
        hashes(&context().redact_line(&marked)),
        hashes(&context().redact_line(&keyed)),
        "{marked}\n{keyed}"
    );
}

/// The small crates' lines (`cmdr-listing`, `cmdr-mtp`), in each site's format string, as a
/// report ships them: the listing folder keeps its kind, and an MTP storage name and upload
/// target hash like the keyed values they replaced.
#[test]
fn the_listing_and_mtp_lines_ship_each_value_as_one_token_of_its_kind() {
    // A listed folder whose name reads like a file: only the site knows it's a folder.
    let dir = "/Users/me/Library/Containers/com.example.app";
    let done = |path: &dyn std::fmt::Display| {
        format!("{HEAD}listing_done listing_id=7 path={path} entries=12 read_dir_ms=3 sort_ms=0 total_ms=4")
    };
    let golden = format!(
        "{HEAD}listing_done listing_id=7 path=$HOME/Library/<dir>/<dir> entries=12 read_dir_ms=3 sort_ms=0 total_ms=4"
    );
    assert_eq!(shape(&done(&LogPrivate::dir_path(dir))), golden);
    assert_ne!(shape(&done(&dir)), golden, "the unmarked spelling guesses the kind");

    // An MTP storage name, marked as a volume.
    let storage = "Alex's SD card (2)";
    let probe =
        |name: &dyn std::fmt::Debug| format!("{HEAD}Storage volumeName={name:?}: write probe succeeded, cleaning up");
    let marked = probe(&LogPrivate::volume(storage));
    assert_ships_none(&marked, &["Alex", "SD", "card"]);
    assert_eq!(
        shape(&marked),
        format!("{HEAD}Storage volumeName=\"<volume>\": write probe succeeded, cleaning up")
    );
    let keyed = probe(&storage);
    assert_eq!(
        hashes(&context().redact_line(&marked)),
        hashes(&context().redact_line(&keyed)),
        "{marked}\n{keyed}"
    );

    // The upload line's folder and file name, each its own value.
    let upload = |dir: &dyn std::fmt::Debug, file: &dyn std::fmt::Debug| {
        format!("{HEAD}Upload failed for dir={dir:?} file={file:?} on mtp-1: Busy")
    };
    let (folder, file) = ("/DCIM/Summer (Lake House) 2026", "IMG Lake 0001.jpg");
    let marked = upload(&LogPrivate::dir_path(folder), &LogPrivate::file_name(file));
    assert_ships_none(&marked, &["Lake", "House", "Summer", "IMG", "0001"]);
    assert_eq!(
        shape(&marked),
        format!("{HEAD}Upload failed for dir=\"/<dir>/<dir>\" file=\"<file>.jpg\" on mtp-1: Busy")
    );
    let keyed = upload(&folder, &file);
    assert_eq!(
        hashes(&context().redact_line(&marked)),
        hashes(&context().redact_line(&keyed)),
        "{marked}\n{keyed}"
    );
}

/// The write engine's lines, in each site's format string, as a report ships them: a prose
/// destination keeps the sentence after it, an operation's source list keeps one token per
/// source, and a share marked as a volume hashes like the keyed value it replaced.
#[test]
fn the_write_engine_lines_ship_each_value_as_one_token_of_its_kind() {
    // A prose destination with parens, then the reason the clash was skipped.
    let dest = "/Volumes/Backup/Summer (Lake House) 2026/notes.txt";
    let skipped = |destination: &dyn std::fmt::Display| {
        format!(
            "{HEAD}OverwriteSmaller (volume): skipping {destination} — destination not strictly smaller (src=12, dst=40)"
        )
    };
    let marked = skipped(&LogPrivate::path(dest));
    assert_ships_none(&marked, &["Lake", "House", "Summer", "notes"]);
    // `src=` and `dst=` are path keys to the regex, so the sizes go too: over-redacted, never
    // leaked.
    let golden = format!(
        "{HEAD}OverwriteSmaller (volume): skipping /Volumes/<volume>/<dir>/<file>.txt — destination not strictly smaller (src=<dir>, dst=<dir>)"
    );
    assert_eq!(shape(&marked), golden);
    assert_ne!(shape(&skipped(&dest)), golden, "the unmarked spelling eats the reason");

    // An operation's source list: each source is its own value.
    let sources = [
        "/Users/me/Pictures/Summer (Lake House) 2026",
        "/Users/me/Pictures/IMG Lake 0001.jpg",
    ];
    let marked = format!(
        "{HEAD}copy_files_start: sources={:?}, destination={:?}, dry_run=false",
        sources.iter().map(LogPrivate::path).collect::<Vec<_>>(),
        LogPrivate::dir_path("/Volumes/Family Photos")
    );
    assert_ships_none(&marked, &["Lake", "House", "Summer", "IMG", "0001", "Family", "Photos"]);
    assert_eq!(
        shape(&marked),
        format!(
            "{HEAD}copy_files_start: sources=[\"$HOME/Pictures/<dir>\", \"$HOME/Pictures/<file>.jpg\"], destination=\"/Volumes/<volume>\", dry_run=false"
        )
    );
    assert_eq!(hashes(&context().redact_line(&marked)).len(), 3, "{marked}");

    // A dropped share, named as the volume it is.
    let share = "Family Photos";
    let outage = |name: &dyn std::fmt::Debug| {
        format!("{HEAD}volume outage: volumeName={name:?} went away; waiting up to 30s for it to come back")
    };
    let marked = outage(&LogPrivate::volume(share));
    assert_ships_none(&marked, &["Family", "Photos"]);
    assert_eq!(
        shape(&marked),
        format!("{HEAD}volume outage: volumeName=\"<volume>\" went away; waiting up to 30s for it to come back")
    );
    let keyed = outage(&share);
    assert_eq!(
        hashes(&context().redact_line(&marked)),
        hashes(&context().redact_line(&keyed)),
        "{marked}\n{keyed}"
    );
}

/// A failed copy's report line carries its typed error as the frontend's JSON in `detail=`
/// (`WriteOperationError::log_detail`, at `operations_host/events.rs` and the write engine's own
/// failure lines), whose identity keys a report tokenizes. Its derived `Debug` shipped
/// `volume_name: "Family Photos"` whole: no report pass knows that key.
#[test]
fn a_copy_failure_ships_no_name_from_its_typed_error() {
    use cmdr_operations::WriteOperationError;
    let errors = [
        WriteOperationError::ServerConnectionLost {
            path: "Photos/Summer (Lake House) Trip/.DS_Store".to_string(),
            volume_name: "Family Photos".to_string(),
            gave_up_after_secs: Some(30),
        },
        WriteOperationError::ReadError {
            path: "/Volumes/Backup/Summer (Lake House) Trip/IMG 0001.jpg".to_string(),
            message: "Permission denied (os error 13)".into(),
        },
    ];
    let private = ["Lake", "House", "Summer", "Trip", "Family", "Photos", "IMG", "0001"];
    for error in errors {
        let volume = format!(
            "{HEAD}copy_between_volumes: operation op-1 failed: detail={:?}",
            error.log_detail()
        );
        assert_ships_none(&volume, &private);
        let local = format!(
            "{HEAD}copy_files_with_progress: failed op=op-1 detail={:?}, keeping 3 completed files",
            error.log_detail()
        );
        assert_ships_none(&local, &private);
        assert!(r(&local).ends_with(", keeping 3 completed files"), "{}", r(&local));
    }
}

/// A failed operation's `detail=` JSON marks every field by its kind, so a share-relative name
/// under a key no report pass knows (`entry`, `other`, `archivePath`, `keptAt`) ships as
/// tokens, and a `message` that kept its volume error typed marks that error's path. The
/// frontend's JSON is the same text with no marker in it.
#[test]
fn a_failed_operations_detail_marks_every_field_by_kind() {
    use cmdr_fs::error_detail::ErrorDetail;
    use cmdr_fs::log_private::RESERVED;
    use cmdr_fs::volume::VolumeError;
    use cmdr_operations::WriteOperationError;

    let share = "Trips/Summer (Lake House) 2026/IMG 0001.jpg";
    let errors = [
        WriteOperationError::ArchiveEntryNamesCollide {
            entry: "Summer Lake/IMG 0001.jpg".to_string(),
            other: "Summer Lake/img 0001.JPG".to_string(),
            archive_path: "Summer Lake House/IMG 0001.jpg".to_string(),
        },
        WriteOperationError::IoError {
            path: "/Volumes/Backup".to_string(),
            message: ErrorDetail::of(VolumeError::PermissionDenied {
                path: share.to_string(),
                raw_os_error: None,
            }),
        },
        WriteOperationError::NewDataKeptAt {
            path: share.to_string(),
            kept_at: "Trips/Summer (Lake House) 2026/IMG 0001 (recovered).jpg".to_string(),
            message: "the rename was refused".into(),
        },
    ];
    let private = ["Lake", "House", "Summer", "IMG", "0001", "recovered"];
    for error in errors {
        let line = format!(
            "{HEAD}copy_between_volumes: operation op-1 failed: detail={:?}",
            error.log_detail()
        );
        assert_ships_none(&line, &private);
        let tag = serde_json::to_value(&error).expect("serializes")["type"].clone();
        let tag = tag.as_str().expect("a tagged enum");
        assert!(
            r(&line).contains(&format!("\\\"type\\\":\\\"{tag}\\\"")),
            "{}",
            r(&line)
        );
        let ipc = serde_json::to_string(&error).expect("serializes");
        assert!(!ipc.contains(|c| RESERVED.contains(&c)), "{ipc}");
    }
}

/// The network backends' lines (`cmdr-smb`, `cmdr-sftp`, `cmdr-s3`), in each site's format
/// string, as a report ships them: share-relative paths keyed on a share keep their kind, a
/// bucket key in prose (no leading `/`, so no regex claims it) ships as tokens, and a slow share
/// named as a volume hashes like the keyed value it replaced.
#[test]
fn the_network_backend_lines_ship_each_value_as_one_token_of_its_kind() {
    // An SMB rename: the share and both share-relative paths, keyed.
    let share = "Family Photos";
    let (from, to) = (
        "Trips/Summer (Lake House) 2026/a.jpg",
        "Trips/Summer (Lake House) 2026/b.jpg",
    );
    let rename = |share: &dyn std::fmt::Debug, from: &dyn std::fmt::Debug, to: &dyn std::fmt::Debug| {
        format!("{HEAD}SmbVolume::rename: share={share:?}, from={from:?}, to={to:?}, force=false")
    };
    let marked = rename(
        &LogPrivate::volume(share),
        &LogPrivate::path(from),
        &LogPrivate::path(to),
    );
    assert_ships_none(&marked, &["Family", "Photos", "Trips", "Summer", "Lake", "House"]);
    assert_eq!(
        shape(&marked),
        format!(
            "{HEAD}SmbVolume::rename: share=\"<volume>\", from=\"<dir>/<dir>/<file>.jpg\", to=\"<dir>/<dir>/<file>.jpg\", force=false"
        )
    );

    // A remote path in prose, on an SFTP rename and an S3 copy-then-delete.
    let sftp = |from: &dyn std::fmt::Display, to: &dyn std::fmt::Display| {
        format!("{HEAD}SftpVolume::rename: {from} → {to}, force=false")
    };
    let marked = sftp(
        &LogPrivate::path("/home/alex/Summer (Lake House)/a.jpg"),
        &LogPrivate::path("/home/alex/Summer (Lake House)/b.jpg"),
    );
    assert_ships_none(&marked, &["alex", "Summer", "Lake", "House"]);
    assert_eq!(
        shape(&marked),
        format!("{HEAD}SftpVolume::rename: $HOME/<dir>/<file>.jpg → $HOME/<dir>/<file>.jpg, force=false")
    );
    let s3 = |from: &dyn std::fmt::Display, to: &dyn std::fmt::Display| {
        format!("{HEAD}s3: {from} was copied to {to} but its delete failed: the server said no")
    };
    let (from, to) = ("Trips/Summer (Lake House)/a.jpg", "Trips/Summer (Lake House)/b.jpg");
    let marked = s3(&LogPrivate::path(from), &LogPrivate::path(to));
    assert_ships_none(&marked, &["Trips", "Summer", "Lake", "House"]);
    let golden = format!(
        "{HEAD}s3: <dir>/<dir>/<file>.jpg was copied to <dir>/<dir>/<file>.jpg but its delete failed: the server said no"
    );
    assert_eq!(shape(&marked), golden);
    assert_ne!(
        shape(&s3(&from, &to)),
        golden,
        "the unmarked spelling ships both keys whole"
    );

    // A slow share, rolled up per share.
    let slow = |share: &dyn std::fmt::Debug| {
        format!(
            "{HEAD}SMB metadata call was slow: share={share:?} get_metadata took 3210 ms. The server held it; check its load (a busy NAS does this under heavy writes)"
        )
    };
    let marked = slow(&LogPrivate::volume(share));
    assert_ships_none(&marked, &["Family", "Photos"]);
    assert_eq!(
        shape(&marked),
        format!(
            "{HEAD}SMB metadata call was slow: share=\"<volume>\" get_metadata took 3210 ms. The server held it; check its load (a busy NAS does this under heavy writes)"
        )
    );
    let keyed = slow(&share);
    assert_eq!(
        hashes(&context().redact_line(&marked)),
        hashes(&context().redact_line(&keyed)),
        "{marked}\n{keyed}"
    );
}
