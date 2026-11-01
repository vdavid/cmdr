use super::*;
use crate::agent::store::proposals::{get_group, page_ops};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::types::{ProposalStatus, ProposalVerb};

const DIR: &str = "/trips/lisbon";

fn file(name: &str) -> FileEntry {
    FileEntry {
        size: Some(100),
        modified_at: Some(1_781_557_809),
        inode: Some(7),
        ..FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
    }
}

fn preview_of(params: Value, folder: &[FileEntry], batch: &[&str]) -> Vec<PreviewRow> {
    let request = parse_request(&params).expect("valid params");
    let compiled = Compiled::new(&request.spec).expect("spec runs");
    let names: Vec<String> = batch.iter().map(|s| (*s).to_string()).collect();
    preview_names(
        &compiled,
        Path::new(DIR),
        &names,
        folder,
        &NameEdits::default(),
        &NoMedia,
    )
}

// ── Params ────────────────────────────────────────────────────────────────────

/// A bare call is a dry run of the sheet's blank settings, which renames nothing.
#[test]
fn a_bare_call_is_a_dry_run_of_the_blank_settings() {
    let request = parse_request(&json!({})).expect("valid");
    assert!(request.dry_run);
    assert_eq!(request.spec.name_mask, "[N]");
    assert_eq!(request.spec.extension_mask, "[E]");
    assert_eq!(
        (request.start, request.limit, request.filter),
        (0, DEFAULT_LIMIT, RowFilter::All)
    );
    let rows = preview_of(json!({}), &[file("a.jpg")], &["a.jpg"]);
    assert_eq!(rows[0].status, RowStatus::Unchanged);
}

/// Every knob the sheet has reaches the spec under its own name.
#[test]
fn every_sheet_knob_is_a_param() {
    let request = parse_request(&json!({
        "nameMask": "[N]_[C]",
        "extensionMask": "[E]",
        "search": "a|b",
        "replace": "x|y",
        "caseSensitive": true,
        "firstOnly": true,
        "includeExtension": true,
        "regex": false,
        "substitute": true,
        "case": "upper",
        "removeDiacritics": true,
        "greekToLatin": true,
        "normalizeUnicode": true,
    }))
    .expect("valid");
    let spec = request.spec;
    assert_eq!(
        (spec.name_mask.as_str(), spec.search.as_str(), spec.replace.as_str()),
        ("[N]_[C]", "a|b", "x|y")
    );
    assert!(spec.case_sensitive && spec.first_only && spec.include_extension && spec.substitute);
    assert!(spec.remove_diacritics && spec.greek_to_latin && spec.normalize_unicode && !spec.regex);
    assert_eq!(spec.case, crate::multi_rename::transform::CaseChange::Upper);
}

#[test]
fn bad_params_are_refused_with_the_field_named() {
    let err = parse_request(&json!({ "case": "sideways" })).expect_err("unknown case");
    assert!(err.message.contains("wrong type"), "{}", err.message);
    parse_request(&json!({ "limit": 0 })).expect_err("zero limit");
    parse_request(&json!({ "limit": MAX_LIMIT + 1 })).expect_err("limit cap");
    parse_request(&json!({ "filter": "ready" })).expect_err("unknown filter");
    parse_request(&json!({ "names": "a.jpg" })).expect_err("names must be an array");
}

/// A media field is a mask like any other: the tool reads the files' EXIF and tags itself.
/// A mask that doesn't parse is still the sheet's typed spec error.
#[test]
fn a_mask_with_media_fields_compiles_and_a_bad_mask_is_a_typed_spec_error() {
    for spec in [
        json!({ "nameMask": "[=exif.taken|modified:YMD] [N]" }),
        json!({ "extensionMask": "[=tag.genre]" }),
    ] {
        let request = parse_request(&spec).expect("valid params");
        assert!(compile(&request.spec).expect("compiles").uses_media(), "{spec}");
    }
    let request = parse_request(&json!({ "nameMask": "[N" })).expect("valid params");
    let err = compile(&request.spec).err().expect("a bad mask");
    assert_eq!(err.data.expect("typed")["specError"]["type"], "nameMask");
}

/// Paging a proposal would silently propose a slice, so it's refused.
#[test]
fn paging_params_beside_a_proposal_are_refused() {
    parse_request(&json!({ "dryRun": false, "limit": 10 })).expect_err("limit is dry-run only");
    parse_request(&json!({ "dryRun": false, "start": 10 })).expect_err("start is dry-run only");
    assert!(!parse_request(&json!({ "dryRun": false })).expect("valid").dry_run);
}

/// The schema has to declare exactly the knobs the parser reads, or a knob is unreachable
/// (the schema gate refuses undeclared keys).
#[test]
fn the_schema_declares_every_spec_knob() {
    let tool = crate::tools::get_all_tools()
        .into_iter()
        .find(|t| t.name == "multi_rename")
        .expect("multi_rename is in the ai-client view");
    let props = tool.input_schema["properties"].as_object().expect("properties").clone();
    for knob in spec_knob_names() {
        assert!(props.contains_key(&knob), "schema is missing '{knob}'");
    }
}

// ── Dry run ───────────────────────────────────────────────────────────────────

/// `start` / `limit` page the preview while the counts cover every row.
#[test]
fn a_dry_run_pages_the_preview_and_counts_every_row() {
    let folder: Vec<FileEntry> = (1..=5).map(|i| file(&format!("img{i}.jpg"))).collect();
    let batch: Vec<String> = folder.iter().map(|f| f.name.clone()).collect();
    let batch: Vec<&str> = batch.iter().map(String::as_str).collect();
    let rows = preview_of(json!({ "nameMask": "trip_[C]" }), &folder, &batch);

    let page = dry_run_page(Path::new(DIR), &rows, 1, 2, RowFilter::All);
    assert_eq!(page.counts.ready, 5);
    assert_eq!((page.total, page.start, page.returned, page.truncated), (5, 1, 2, true));
    let names: Vec<_> = page
        .rows
        .iter()
        .map(|r| (r.old_name.as_str(), r.new_name.as_str()))
        .collect();
    assert_eq!(names, vec![("img2.jpg", "trip_2.jpg"), ("img3.jpg", "trip_3.jpg")]);

    let last = dry_run_page(Path::new(DIR), &rows, 4, 2, RowFilter::All);
    assert_eq!((last.returned, last.truncated), (1, false));
}

/// The problems filter lists the rows the sheet would flag, and nothing else.
#[test]
fn the_problems_filter_keeps_only_flagged_rows() {
    let folder = vec![file("a.jpg"), file("b.jpg"), file("taken.jpg")];
    // `a` and `b` both become `taken.jpg`, which a file that stays already holds.
    let rows = preview_of(json!({ "nameMask": "taken" }), &folder, &["a.jpg", "b.jpg"]);
    let page = dry_run_page(Path::new(DIR), &rows, 0, 50, RowFilter::Problems);
    assert_eq!(page.counts.problems, 2);
    assert_eq!(page.total, 2);
    assert!(page.rows.iter().all(|r| r.status.is_problem()));
}

// ── Proposal ──────────────────────────────────────────────────────────────────

fn migrated_conn() -> rusqlite::Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// The non-dry run stages ONLY the ready rows as one pending rename group, each carrying what
/// its file looked like, and renames nothing.
#[test]
fn a_proposal_stages_the_ready_rows_as_one_pending_rename_group() {
    let folder = vec![file("a.jpg"), file("b.jpg"), file("keep.txt")];
    let rows = preview_of(
        json!({ "nameMask": "[N]_lisbon" }),
        &folder,
        &["a.jpg", "b.jpg", "gone.jpg"],
    );
    assert_eq!(Counts::of(&rows).problems, 1, "gone.jpg is Missing");

    let group = rename_group("root", Path::new(DIR), &rows, &folder, Some("Tag the trip".into())).expect("ready rows");
    let conn = migrated_conn();
    let sweep = NewSweep {
        conversation_id: None,
        created_by_model: None,
        rationale: None,
    };
    let proposed = crate::agent::suggested_ops::propose(&conn, &sweep, std::slice::from_ref(&group), 1_800_000_000)
        .expect("proposed");
    let group_id = proposed.group_ids[0];

    let stored = get_group(&conn, group_id).expect("read").expect("group exists");
    assert_eq!(stored.verb, ProposalVerb::Rename);
    assert_eq!(stored.status, ProposalStatus::Pending);
    assert_eq!(stored.destination.as_deref(), Some(DIR));
    assert_eq!(stored.display_name, "Rename two files in lisbon");

    let ops = page_ops(&conn, group_id, 100, 0).expect("ops");
    let pairs: Vec<_> = ops
        .iter()
        .map(|op| (op.source_path.as_str(), op.destination.as_deref()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("/trips/lisbon/a.jpg", Some("a_lisbon.jpg")),
            ("/trips/lisbon/b.jpg", Some("b_lisbon.jpg"))
        ]
    );
    assert_eq!(
        (ops[0].snapshot_size, ops[0].snapshot_mtime, ops[0].snapshot_inode),
        (Some(100), Some(1_781_557_809), Some(7))
    );
}

#[test]
fn a_batch_with_nothing_ready_is_refused_rather_than_staged_empty() {
    let folder = vec![file("a.jpg")];
    let rows = preview_of(json!({}), &folder, &["a.jpg"]);
    assert_eq!(
        rename_group("root", Path::new(DIR), &rows, &folder, None).expect_err("nothing ready"),
        ProposalRefusal::NothingReady
    );
}

// ── Media fields ──────────────────────────────────────────────────────────────

mod media {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use chrono::NaiveDate;
    use cmdr_listing::testing::{TestListing, TestListingGuard};
    use cmdr_local::reading::list_directory_core;
    use cmdr_media_meta::{MediaFacts, MediaField, ReadError};

    use super::*;
    use crate::ignore_poison::IgnorePoison;
    use crate::multi_rename::media::{FileJob, Loader};
    use crate::multi_rename::session;
    use crate::test_support::{TestDir, wait_until, wait_until_async};

    /// Under nextest's 8 s limit, so a stuck read fails with the wait's message.
    const WAIT: Duration = Duration::from_secs(5);

    /// A scratch folder with `files`, its listing entries, and a cached listing on `root`.
    fn folder(tag: &str, files: &[&str]) -> (TestDir, Vec<FileEntry>, TestListingGuard) {
        let dir = TestDir::new(tag);
        for name in files {
            std::fs::write(dir.join(name), b"x").expect("scratch dir is writable");
        }
        let entries = list_directory_core(&dir, &crate::listing_host::backends::AppListings).expect("the dir lists");
        let listing = TestListing::new()
            .volume("root")
            .path(dir.to_path_buf())
            .entries(entries.clone())
            .insert(crate::listing_host::listings(), tag);
        (dir, entries, listing)
    }

    /// Which files a reader was handed.
    #[derive(Clone, Default)]
    struct Calls(Arc<Mutex<Vec<String>>>);

    impl Calls {
        fn is_empty(&self) -> bool {
            self.0.lock_ignore_poison().is_empty()
        }
    }

    fn loader(
        calls: &Calls,
        answer: impl Fn(&FileJob) -> Result<MediaFacts, ReadError> + Send + Sync + 'static,
    ) -> Loader {
        let calls = calls.clone();
        Loader {
            reader: Arc::new(move |job: &FileJob| {
                calls.0.lock_ignore_poison().push(job.name.clone());
                answer(job)
            }),
            emit: Arc::new(|_| {}),
            parallel: 4,
            volume_gone: Arc::new(|| false),
        }
    }

    /// A photo taken on 2024-05-06, a song titled "Harder", one that won't parse, and
    /// one with nothing in it.
    fn facts(job: &FileJob) -> Result<MediaFacts, ReadError> {
        match job.name.as_str() {
            "photo.jpg" => Ok(MediaFacts {
                taken: NaiveDate::from_ymd_opt(2024, 5, 6).and_then(|d| d.and_hms_opt(7, 8, 9)),
                ..MediaFacts::default()
            }),
            "song.mp3" => Ok(MediaFacts {
                title: Some("Harder".to_string()),
                ..MediaFacts::default()
            }),
            "broken.jpg" => Err(ReadError::Io("not a JPEG".to_string())),
            _ => Ok(MediaFacts::default()),
        }
    }

    const FILES: [&str; 5] = ["photo.jpg", "song.mp3", "broken.jpg", "blank.jpg", "notes.txt"];

    /// The folder's files in the listing's order, which is the order a sheet over every row renames in.
    fn names(entries: &[FileEntry]) -> Vec<String> {
        entries.iter().map(|e| e.name.clone()).collect()
    }

    fn specs() -> Vec<MultiRenameSpec> {
        [
            json!({ "nameMask": "[=exif.taken:YMD] [N]" }),
            json!({ "nameMask": "[=exif.taken|modified:YMD] [N]" }),
            json!({ "nameMask": "[=tag.title]" }),
        ]
        .iter()
        .map(|params| parse_request(params).expect("valid params").spec)
        .collect()
    }

    /// Opens a sheet session over the folder and reads it through with `loader`.
    async fn read_sheet(listing: &TestListingGuard, loader: Loader) -> String {
        let id = session::open(listing.id(), true, None, 0)
            .expect("the session opens")
            .session_id;
        session::start_media_with(&id, loader).expect("the read starts");
        wait_until_async(WAIT, "the sheet's media read", || {
            session::media_progress(&id).is_ok_and(|p| p.done)
        })
        .await;
        id
    }

    /// The tool's preview is the sheet's, row for row: same values, same `NoValue` and
    /// `Unreadable`, same `|modified` fallback.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_media_mask_previews_exactly_as_the_sheet_does() {
        let (dir, entries, listing) = folder("mcp-rename-media-same", &FILES);
        let sheet = read_sheet(&listing, loader(&Calls::default(), facts)).await;

        let media = read_media(
            &dir,
            &names(&entries),
            &entries,
            &[],
            loader(&Calls::default(), facts),
            MAX_MEDIA_READS,
        )
        .await
        .expect("the tool reads");

        for spec in specs() {
            let compiled = compile(&spec).expect("compiles");
            let ours = preview(&compiled, &dir, &names(&entries), &entries, Some(&media));
            let theirs = session::preview_session(&sheet, &spec)
                .expect("the sheet previews")
                .rows;
            assert_eq!(ours, theirs, "{}", spec.name_mask);
        }
        let compiled = compile(&specs()[0]).expect("compiles");
        let rows = preview(&compiled, &dir, &names(&entries), &entries, Some(&media));
        let mut statuses: Vec<_> = rows.iter().map(|r| (r.old_name.as_str(), r.status.clone())).collect();
        statuses.sort_by_key(|(name, _)| *name);
        let field = MediaField::ExifTaken;
        assert_eq!(
            statuses,
            vec![
                ("blank.jpg", RowStatus::NoValue { field }),
                ("broken.jpg", RowStatus::Unreadable { field }),
                ("notes.txt", RowStatus::NoValue { field }),
                ("photo.jpg", RowStatus::Ready),
                ("song.mp3", RowStatus::NoValue { field }),
            ]
        );
        session::close(&sheet);
    }

    /// An open sheet already read the folder: the tool takes its values and reads nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_open_sheets_reads_are_reused_rather_than_read_again() {
        let (dir, entries, listing) = folder("mcp-rename-media-reuse", &FILES);
        let sheet = read_sheet(&listing, loader(&Calls::default(), facts)).await;
        let open = session::media_over("root", &dir);
        assert_eq!(open.len(), 1, "the sheet's session is over this folder");

        let calls = Calls::default();
        let media = read_media(
            &dir,
            &names(&entries),
            &entries,
            &open,
            loader(&calls, |_| Ok(MediaFacts::default())),
            1,
        )
        .await
        .expect("nothing left to read, so the cap doesn't bite");

        assert!(calls.is_empty(), "read nothing");
        let spec = &specs()[2];
        let compiled = compile(spec).expect("compiles");
        assert_eq!(
            preview(&compiled, &dir, &names(&entries), &entries, Some(&media)),
            session::preview_session(&sheet, spec).expect("the sheet previews").rows
        );
        session::close(&sheet);
    }

    /// Past the cap of files left to read, the tool refuses, typed, before reading any.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn more_media_files_to_read_than_the_cap_is_a_typed_refusal() {
        let (dir, entries, _listing) = folder("mcp-rename-media-cap", &FILES);
        let calls = Calls::default();

        let err = read_media(&dir, &names(&entries), &entries, &[], loader(&calls, facts), 3)
            .await
            .err()
            .expect("four media files, a cap of three");

        assert_eq!(
            err.data,
            Some(json!({ "specError": { "type": "tooManyMediaFiles", "files": 4, "max": 3 } }))
        );
        assert!(calls.is_empty(), "read nothing");
    }

    /// A call that goes away (the client hung up) stops its read at the next fetch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_dropped_call_cancels_its_read() {
        let (dir, entries, _listing) = folder("mcp-rename-media-cancel", &["slow.jpg"]);
        let seen: Arc<Mutex<Option<Arc<AtomicBool>>>> = Arc::default();
        let reader_saw = Arc::clone(&seen);
        let slow = loader(&Calls::default(), move |job| {
            *reader_saw.lock_ignore_poison() = Some(Arc::clone(&job.cancel));
            // On a blocking thread, so the sync wait is fine; bounded, so a broken cancel can't hang the runtime's
            // shutdown (its panic makes the file unreadable, and the assert below fails).
            wait_until(WAIT, "the call's cancel", || job.cancel.load(Ordering::Relaxed));
            Err(ReadError::Cancelled)
        });
        let names = vec!["slow.jpg".to_string()];

        let call = read_media(&dir, &names, &entries, &[], slow, MAX_MEDIA_READS);
        let timed_out = tokio::time::timeout(Duration::from_millis(200), call).await;

        assert!(timed_out.is_err(), "the read was still going");
        let cancel = seen.lock_ignore_poison().clone().expect("the reader started");
        assert!(cancel.load(Ordering::Relaxed), "dropping the call cancels the read");
    }

    // ── The sensitive-content gate ──────────────────────────────────────────────

    /// A cached listing over `dir/.ssh`, holding `files`.
    fn protected_folder(dir: &TestDir, tag: &str, files: &[&str]) -> (PathBuf, Vec<FileEntry>, TestListingGuard) {
        let folder = dir.join(".ssh");
        std::fs::create_dir(&folder).expect("scratch dir is writable");
        for name in files {
            std::fs::write(folder.join(name), b"x").expect("scratch dir is writable");
        }
        let entries = list_directory_core(&folder, &crate::listing_host::backends::AppListings).expect("the dir lists");
        let listing = TestListing::new()
            .volume("root")
            .path(folder.clone())
            .entries(entries.clone())
            .insert(crate::listing_host::listings(), tag);
        (folder, entries, listing)
    }

    fn title_mask() -> Compiled {
        compile(
            &parse_request(&json!({ "nameMask": "[=tag.title]" }))
                .expect("valid")
                .spec,
        )
        .expect("compiles")
    }

    /// `tag.comment` and `tag.title` are file-derived text, so a media row in a protected
    /// folder doesn't read the file at all.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_media_row_on_a_protected_path_reads_nothing() {
        let dir = TestDir::new("mcp-rename-gate");
        let (folder, entries, _listing) = protected_folder(&dir, "mcp-rename-gate", &["song.mp3"]);
        let calls = Calls::default();
        let refused = RefusedReads::default();

        let media = read_media(
            &folder,
            &names(&entries),
            &entries,
            &[],
            gated(loader(&calls, facts), &refused),
            MAX_MEDIA_READS,
        )
        .await
        .expect("the tool reads");

        assert!(calls.is_empty(), "the gate comes before the file is opened");
        assert_eq!(refused.take(), vec![Protected::Credentials]);
        let rows = preview(&title_mask(), &folder, &names(&entries), &entries, Some(&media));
        assert_eq!(
            rows[0].status,
            RowStatus::Unreadable {
                field: MediaField::TagTitle
            }
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_symlink_to_a_protected_file_reads_nothing() {
        let (dir, _, _listing) = folder("mcp-rename-gate-link", &[]);
        std::fs::create_dir(dir.join(".ssh")).unwrap();
        std::fs::write(dir.join(".ssh/song.mp3"), b"x").unwrap();
        std::os::unix::fs::symlink(dir.join(".ssh/song.mp3"), dir.join("song.mp3")).unwrap();
        let entries = list_directory_core(&dir, &crate::listing_host::backends::AppListings).unwrap();
        let wanted = vec!["song.mp3".to_string()];
        let calls = Calls::default();
        let refused = RefusedReads::default();

        read_media(
            &dir,
            &wanted,
            &entries,
            &[],
            gated(loader(&calls, facts), &refused),
            MAX_MEDIA_READS,
        )
        .await
        .expect("the tool reads");

        assert!(calls.is_empty());
        assert_eq!(refused.take(), vec![Protected::Credentials]);
    }

    /// An open sheet read those files for a person. Taking its answers would walk straight past
    /// the gate, since no reader runs for them.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn answers_an_open_sheet_read_past_the_gate_are_not_taken() {
        let dir = TestDir::new("mcp-rename-gate-sheet");
        let (folder, entries, listing) = protected_folder(&dir, "mcp-rename-gate-sheet", &["song.mp3"]);
        let sheet = read_sheet(&listing, loader(&Calls::default(), facts)).await;
        let open = session::media_over("root", &folder);
        assert_eq!(open.len(), 1, "the sheet's session is over this folder");
        let calls = Calls::default();
        let refused = RefusedReads::default();

        let media = read_media(
            &folder,
            &names(&entries),
            &entries,
            &open,
            gated(loader(&calls, facts), &refused),
            MAX_MEDIA_READS,
        )
        .await
        .expect("the tool reads");

        assert_eq!(refused.take(), vec![Protected::Credentials]);
        let rows = preview(&title_mask(), &folder, &names(&entries), &entries, Some(&media));
        assert_ne!(
            rows[0].new_name, "Harder.mp3",
            "the sheet's answer stayed with the sheet"
        );
        assert_eq!(
            rows[0].status,
            RowStatus::Unreadable {
                field: MediaField::TagTitle
            }
        );
        session::close(&sheet);
    }

    /// A folder's worth of refusals is no sign the volume is dying: the rest still read.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn refused_rows_do_not_make_the_loader_give_up_on_the_volume() {
        let (dir, _, _listing) = folder("mcp-rename-gate-many", &["photo.jpg"]);
        std::fs::create_dir(dir.join(".ssh")).unwrap();
        let mut wanted = Vec::new();
        for n in 0..6 {
            let name = format!("link{n}.jpg");
            std::fs::write(dir.join(".ssh").join(&name), b"x").unwrap();
            std::os::unix::fs::symlink(dir.join(".ssh").join(&name), dir.join(&name)).unwrap();
            wanted.push(name);
        }
        wanted.push("photo.jpg".to_string());
        let entries = list_directory_core(&dir, &crate::listing_host::backends::AppListings).unwrap();
        let calls = Calls::default();
        let refused = RefusedReads::default();
        let serial = Loader {
            parallel: 1,
            ..loader(&calls, facts)
        };

        read_media(&dir, &wanted, &entries, &[], gated(serial, &refused), MAX_MEDIA_READS)
            .await
            .expect("the tool reads");

        assert_eq!(refused.take().len(), 6);
        assert_eq!(*calls.0.lock_ignore_poison(), vec!["photo.jpg".to_string()]);
    }
}
