//! The search goldens: today's answers end to end, pinned as JSON, so moving the
//! engine into its own crate can prove it changed nothing.
//!
//! A fixed corpus goes into a real index database (`IndexStore`), the arena loads
//! through the production loader, and every case enters through ONE function,
//! [`golden_run`], whose signature stays fixed while its body follows the code. So
//! the proof isn't rewritten by the change it proves. A golden that moves is a bug,
//! ❌ never a re-record, except the deliberate behavior changes the search-crate
//! plan registers (`docs/specs/search-crate-plan.md` § "Deliberate behavior changes").
//!
//! - **Hermetic**: the corpus sits under `/cmdr-golden` and `/Volumes/cmdr-golden-share`,
//!   paths that exist nowhere and live under no `$HOME`, so `realpath` keeps them
//!   literal and the `~` substitution never fires. Weights go in as an argument,
//!   ❌ never through `volumes::weights_for`'s static.
//! - **Id order**: rows insert root first, then depth-first with each folder's
//!   children in byte order of their names, the order an in-memory source assigns
//!   ids in, so the two can be required to agree on `entry_id`s.
//! - **Per platform**: case folding and NFD normalization differ, so each platform
//!   has its own file (`goldens/macos.json`, `goldens/linux.json`).
//! - **Recording**: `CMDR_RECORD_SEARCH_GOLDENS=1` rewrites this platform's file.
//!   Only for a new case or a registered behavior change, said in the commit.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use cmdr_index::ReadPool;
use cmdr_index::store::{DirStatsById, IndexStore, ROOT_ID};
use serde_json::{Value, json};

use super::*;
use crate::search::context::{NameRules, SearchContext};
use crate::search::index::load_search_index;
use crate::search::live::events::CollectorSearchEventSink;
use crate::search::live::{
    CoverageFacts, CoverageKind, LiveRun, ResultStream, SearchPhase, UnresolvedScope, WalkEnding,
};
use crate::search::types::SearchSort;

use super::super::live_run::covered_half_for_run;

/// The mount-rooted volume's mount root.
const SHARE_ROOT: &str = "/Volumes/cmdr-golden-share";
const SHARE_VOLUME_ID: &str = "smb-cmdr-golden-share";

// ── The corpus ───────────────────────────────────────────────────────

/// One row of a corpus: its path below the index root, whether it's a folder, and
/// its size and date (`None` is a NULL column: a hardlink-deduped size, an unknown
/// date). A folder's size is its `dir_stats` row, summed from the files below it.
struct Row {
    path: &'static str,
    is_directory: bool,
    size: Option<u64>,
    modified_at: Option<u64>,
}

const fn dir(path: &'static str, modified_at: u64) -> Row {
    Row {
        path,
        is_directory: true,
        size: None,
        modified_at: Some(modified_at),
    }
}

const fn file(path: &'static str, size: Option<u64>, modified_at: Option<u64>) -> Row {
    Row {
        path,
        is_directory: false,
        size,
        modified_at,
    }
}

/// The boot volume's tree. Every folder is listed before what it holds.
fn root_corpus() -> Vec<Row> {
    vec![
        dir("/cmdr-golden", 1_700_000_000),
        dir("/cmdr-golden/Archive", 978_307_200),
        file("/cmdr-golden/Archive/old-report.zip", Some(2_000), Some(978_307_200)),
        dir("/cmdr-golden/Docs", 1_750_000_000),
        file("/cmdr-golden/Docs/Report.PDF", Some(50_000), Some(1_700_000_000)),
        file("/cmdr-golden/Docs/big.iso", Some(5_000_000_000), Some(1_750_000_000)),
        file("/cmdr-golden/Docs/hardlinked.bin", None, Some(1_700_000_100)),
        file("/cmdr-golden/Docs/line\nbreak.txt", Some(12), Some(1_700_000_200)),
        // NFD: `e` plus a combining acute, as a Mac's older APIs write it.
        file(
            "/cmdr-golden/Docs/re\u{301}sume\u{301}.txt",
            Some(300),
            Some(1_700_000_300),
        ),
        file("/cmdr-golden/Docs/undated.txt", Some(5), None),
        dir("/cmdr-golden/Library", 1_760_000_000),
        dir("/cmdr-golden/Library/Caches", 1_760_000_000),
        file(
            "/cmdr-golden/Library/Caches/thumb-report.db",
            Some(4_096),
            Some(1_760_000_000),
        ),
        dir("/cmdr-golden/Photos", 1_720_000_000),
        // NFC, as a browser or a newer API writes it.
        dir("/cmdr-golden/Photos/Caf\u{e9}", 1_720_000_000),
        file(
            "/cmdr-golden/Photos/Caf\u{e9}/caf\u{e9}-terrace.jpg",
            Some(2_500_000),
            Some(1_720_000_000),
        ),
        file(
            "/cmdr-golden/Photos/r\u{e9}sum\u{e9}.txt",
            Some(400),
            Some(1_710_000_000),
        ),
        dir("/cmdr-golden/Projects", 1_762_000_000),
        dir("/cmdr-golden/Projects/cmdr", 1_761_600_000),
        dir("/cmdr-golden/Projects/cmdr/.git", 1_760_000_100),
        file("/cmdr-golden/Projects/cmdr/.git/HEAD", Some(23), Some(1_760_000_100)),
        file("/cmdr-golden/Projects/cmdr/README.md", Some(1_500), Some(1_761_000_000)),
        dir("/cmdr-golden/Projects/cmdr/node_modules", 1_600_000_000),
        dir("/cmdr-golden/Projects/cmdr/node_modules/left-pad", 1_600_000_000),
        file(
            "/cmdr-golden/Projects/cmdr/node_modules/left-pad/index.js",
            Some(800),
            Some(1_600_000_000),
        ),
        dir("/cmdr-golden/Projects/cmdr/src", 1_761_600_000),
        file(
            "/cmdr-golden/Projects/cmdr/src/main.rs",
            Some(3_000),
            Some(1_761_500_000),
        ),
        file(
            "/cmdr-golden/Projects/cmdr/src/search.rs",
            Some(9_000),
            Some(1_761_600_000),
        ),
        file(
            "/cmdr-golden/Projects/report-2026.pdf",
            Some(70_000),
            Some(1_762_000_000),
        ),
        file("/cmdr-golden/Projects/report.pdf", Some(10_000), Some(1_740_000_000)),
        dir("/cmdr-golden/line\nfolder", 1_700_000_400),
        file("/cmdr-golden/line\nfolder/inside.txt", Some(7), Some(1_700_000_400)),
        dir("/cmdr-golden/unsized-folder", 1_700_000_500),
        file("/cmdr-golden/unsized-folder/note.md", Some(100), Some(1_700_000_500)),
    ]
}

/// The folder with no `dir_stats` row: unknown-sized, which sorts last and fails a
/// size filter.
const UNSIZED_FOLDER: &str = "/cmdr-golden/unsized-folder";

/// The share's tree, mount-relative as a non-root index stores it.
fn share_corpus() -> Vec<Row> {
    vec![
        dir("/papers", 1_705_000_000),
        dir("/papers/invoices paid", 1_700_000_000),
        file("/papers/invoices paid/hetzner.pdf", Some(10), Some(1_700_000_000)),
        file("/papers/report.txt", Some(20), Some(1_705_000_000)),
        dir("/photos", 1_715_000_000),
        file("/photos/IMG_0001.JPG", Some(3_000_000), Some(1_715_000_000)),
    ]
}

fn root_weights() -> ImportanceWeights {
    let mut weights = ImportanceWeights::empty();
    weights.insert("/cmdr-golden/Projects/cmdr", 0.9);
    weights.insert("/cmdr-golden/Docs", 0.4);
    weights
}

fn share_weights() -> ImportanceWeights {
    let mut weights = ImportanceWeights::empty();
    weights.insert("/Volumes/cmdr-golden-share/papers", 0.6);
    weights
}

/// Write `rows` into a fresh index at `db_path` in the defined id order, plus a
/// `dir_stats` row per folder (except [`UNSIZED_FOLDER`]).
fn write_corpus(db_path: &Path, rows: &[Row]) {
    let _store = IndexStore::open(db_path).expect("open the golden index");
    let conn = IndexStore::open_write_connection(db_path).expect("a write connection");

    let mut children: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        let parent = row.path.rsplit_once('/').map_or("", |(parent, _)| parent);
        children.entry(parent).or_default().push(row);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|a, b| name_of(a.path).as_bytes().cmp(name_of(b.path).as_bytes()));
    }

    // Depth-first, preorder: a folder's whole subtree before its next sibling.
    fn insert<'a>(
        conn: &rusqlite::Connection,
        parent_id: i64,
        parent_path: &str,
        children: &BTreeMap<&str, Vec<&'a Row>>,
        ids: &mut Vec<(i64, &'a str)>,
    ) {
        for row in children.get(parent_path).into_iter().flatten() {
            let id = IndexStore::insert_entry_v2(
                conn,
                parent_id,
                name_of(row.path),
                row.is_directory,
                false,
                row.size,
                row.size,
                row.modified_at,
                None,
            )
            .expect("insert a golden row");
            ids.push((id, row.path));
            if row.is_directory {
                insert(conn, id, row.path, children, ids);
            }
        }
    }
    let mut ids: Vec<(i64, &str)> = Vec::new();
    insert(&conn, ROOT_ID, "", &children, &mut ids);

    let stats: Vec<DirStatsById> = ids
        .iter()
        .filter_map(|&(id, path)| {
            let row = rows.iter().find(|r| r.path == path)?;
            (row.is_directory && path != UNSIZED_FOLDER).then(|| {
                let below = format!("{path}/");
                let files = rows.iter().filter(|r| !r.is_directory && r.path.starts_with(&below));
                let (count, bytes) = files.fold((0, 0), |(n, b), r| (n + 1, b + r.size.unwrap_or(0)));
                DirStatsById {
                    entry_id: id,
                    recursive_logical_size: bytes,
                    recursive_physical_size: bytes,
                    recursive_file_count: count,
                    ..Default::default()
                }
            })
        })
        .collect();
    IndexStore::upsert_dir_stats_by_id(&conn, &stats).expect("write golden dir_stats");
}

fn name_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// One corpus loaded the way production loads it: rows through `load_search_index`.
fn load(dir: &Path, file_name: &str, rows: &[Row], mount_root: Option<&str>) -> LoadedVolume {
    let db_path = dir.join(file_name);
    write_corpus(&db_path, rows);
    let pool = ReadPool::new(db_path).expect("a read pool over the golden index");
    let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load the golden arena");
    LoadedVolume::for_test(index, pool, mount_root.map(str::to_string))
}

// ── The entry point ──────────────────────────────────────────────────

/// One golden answer: the covered half of a search, as `run_blocking` returns it,
/// with each row's `entry_id` (which the wire skips) kept in.
///
/// ❌ Keep the signature: only the body follows the code.
fn golden_run(
    query: &SearchQuery,
    target: &Target,
    loaded: &LoadedVolume,
    weights: &ImportanceWeights,
) -> Result<Value, String> {
    golden_run_under(NameRules::host(), None, query, target, loaded, weights)
}

/// [`golden_run`] under given name rules, so one platform's goldens can be checked on
/// another, and with a home folder for the `~` cases (the rest pass `None`: the
/// corpus lives under no real home).
fn golden_run_under(
    rules: NameRules,
    home_dir: Option<&str>,
    query: &SearchQuery,
    target: &Target,
    loaded: &LoadedVolume,
    weights: &ImportanceWeights,
) -> Result<Value, String> {
    let ctx = SearchContext {
        rules,
        home_dir,
        weights,
    };
    let half = search_covered_half(query, target, loaded, &ctx).map_err(|e| e.to_string())?;
    Ok(json!({
        "entries": half.entries.iter().map(row_json).collect::<Vec<_>>(),
        "totalCount": half.total,
        "unresolvedScopes": half.unresolved_scopes,
        "hiddenByExcludes": half.hidden_by_excludes,
    }))
}

/// A live run over ground the index covers whole: the covered half goes out, then
/// the terminal report with nothing to walk. Pins the rows the events carried and
/// the terminal event, ❌ not the batch splits (a flush interval drives those).
fn golden_live_trace(
    rules: NameRules,
    query: &SearchQuery,
    target: &Target,
    loaded: &LoadedVolume,
    weights: &ImportanceWeights,
) -> Value {
    let ctx = SearchContext {
        rules,
        home_dir: None,
        weights,
    };
    let run = LiveRun::detached("golden-run", &target.volume_id);
    let sink = CollectorSearchEventSink::default();
    let mut stream = ResultStream::new(&run, &sink, query);
    stream.announce(SearchPhase::ResolvingCoverage);
    match covered_half_for_run(query, target, loaded, &ctx) {
        Ok(half) => {
            stream.add_indexed(half.entries, half.total, half.hidden_by_excludes);
            // The same report a desktop run with nothing to walk ends on.
            stream.finish(CoverageFacts {
                unresolved_scopes: half
                    .unresolved_scopes
                    .into_iter()
                    .map(|path| UnresolvedScope { path, walked: false })
                    .collect(),
                ..CoverageFacts::new(
                    WalkEnding::NothingToWalk,
                    CoverageKind::Covered,
                    target.volume_id.clone(),
                )
            });
        }
        Err((error, message)) => stream.fail(error, message),
    }
    use crate::ignore_poison::IgnorePoison;
    json!({
        "rows": sink.rows().iter().map(row_json).collect::<Vec<_>>(),
        "complete": serde_json::to_value(&*sink.complete.lock_ignore_poison()).expect("complete events serialize"),
        "cancelled": serde_json::to_value(&*sink.cancelled.lock_ignore_poison()).expect("cancelled events serialize"),
        "errors": serde_json::to_value(&*sink.errors.lock_ignore_poison()).expect("error events serialize"),
    })
}

fn row_json(entry: &SearchResultEntry) -> Value {
    let mut value = serde_json::to_value(entry).expect("a result row serializes");
    value["entryId"] = json!(entry.entry_id);
    value
}

// ── The cases ────────────────────────────────────────────────────────

/// A query with every field at its default: no filters, the platform's case rule,
/// system folders shown.
fn base() -> SearchQuery {
    SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

fn named(pattern: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(pattern.to_string()),
        ..base()
    }
}

fn scoped(mut query: SearchQuery, paths: &[&str]) -> SearchQuery {
    query.include_paths = Some(paths.iter().map(|p| p.to_string()).collect());
    query
}

#[derive(Clone, Copy)]
enum Volume {
    Root,
    Share,
}

fn cases() -> Vec<(&'static str, Volume, SearchQuery)> {
    use Volume::{Root, Share};
    let regex = |p: &str| SearchQuery {
        pattern_type: PatternType::Regex,
        ..named(p)
    };
    let all = || named("*");
    vec![
        ("glob_plain", Root, named("report")),
        ("glob_wildcard_extension", Root, named("*.pdf")),
        ("glob_question_marks", Root, named("report-????.pdf")),
        ("regex_anchored", Root, regex("^re.*\\.pdf$")),
        ("regex_invalid", Root, regex("[unclosed")),
        ("case_platform_default", Root, named("REPORT")),
        (
            "case_sensitive_on",
            Root,
            SearchQuery {
                case_sensitive: Some(true),
                ..named("Report")
            },
        ),
        (
            "case_sensitive_off",
            Root,
            SearchQuery {
                case_sensitive: Some(false),
                ..named("REPORT.pdf")
            },
        ),
        ("accented_nfc_pattern", Root, named("r\u{e9}sum\u{e9}")),
        ("accented_nfd_pattern", Root, named("re\u{301}sume\u{301}")),
        ("accented_folder", Root, named("caf\u{e9}")),
        ("newline_in_name", Root, named("line\nbreak*")),
        ("under_a_newline_folder", Root, named("inside")),
        (
            "files_at_least_1_mb",
            Root,
            SearchQuery {
                min_size: Some(1_000_000),
                is_directory: Some(false),
                ..base()
            },
        ),
        (
            "files_at_most_100_b",
            Root,
            SearchQuery {
                max_size: Some(100),
                is_directory: Some(false),
                ..base()
            },
        ),
        (
            "size_range_any_kind",
            Root,
            SearchQuery {
                min_size: Some(1_000),
                max_size: Some(100_000),
                ..base()
            },
        ),
        (
            "modified_after",
            Root,
            SearchQuery {
                modified_after: Some(1_750_000_000),
                ..base()
            },
        ),
        (
            "modified_before",
            Root,
            SearchQuery {
                modified_before: Some(1_000_000_000),
                ..base()
            },
        ),
        (
            "modified_range",
            Root,
            SearchQuery {
                modified_after: Some(1_700_000_000),
                modified_before: Some(1_710_000_000),
                ..all()
            },
        ),
        (
            "folders_only",
            Root,
            SearchQuery {
                is_directory: Some(true),
                ..base()
            },
        ),
        (
            "folder_size_filter",
            Root,
            SearchQuery {
                min_size: Some(1_000_000),
                ..base()
            },
        ),
        (
            "folders_by_size",
            Root,
            SearchQuery {
                is_directory: Some(true),
                sort_by: Some(SearchSort::Size),
                ..base()
            },
        ),
        (
            "all_by_size",
            Root,
            SearchQuery {
                sort_by: Some(SearchSort::Size),
                ..all()
            },
        ),
        (
            "all_by_modified",
            Root,
            SearchQuery {
                sort_by: Some(SearchSort::Modified),
                ..all()
            },
        ),
        (
            "scope_one_folder",
            Root,
            scoped(named("report"), &["/cmdr-golden/Projects"]),
        ),
        (
            "scope_two_folders",
            Root,
            scoped(all(), &["/cmdr-golden/Docs", "/cmdr-golden/Photos"]),
        ),
        (
            "scope_unresolved",
            Root,
            scoped(named("report"), &["/cmdr-golden/Nope"]),
        ),
        (
            "scope_half_resolved",
            Root,
            scoped(all(), &["/cmdr-golden/Docs", "/cmdr-golden/Nope"]),
        ),
        ("scope_whole_root_volume", Root, scoped(named("report"), &["/"])),
        (
            "system_dirs_hidden",
            Root,
            SearchQuery {
                exclude_system_dirs: None,
                ..all()
            },
        ),
        ("system_dirs_shown", Root, all()),
        (
            "exclude_a_name",
            Root,
            SearchQuery {
                exclude_dir_names: Some(vec!["Docs".into()]),
                ..all()
            },
        ),
        (
            "exclude_a_glob",
            Root,
            SearchQuery {
                exclude_dir_names: Some(vec!["Pho*".into()]),
                ..all()
            },
        ),
        (
            "exclude_a_path",
            Root,
            SearchQuery {
                exclude_dir_names: Some(vec!["/cmdr-golden/Projects/cmdr".into()]),
                ..all()
            },
        ),
        (
            "exclude_unusable_glob",
            Root,
            SearchQuery {
                exclude_dir_names: Some(vec![format!("{}*", "?".repeat(200_000))]),
                ..all()
            },
        ),
        (
            "count_only",
            Root,
            SearchQuery {
                count_only: true,
                ..all()
            },
        ),
        (
            "count_only_folder_size",
            Root,
            SearchQuery {
                count_only: true,
                min_size: Some(1_000_000),
                ..base()
            },
        ),
        ("limit_three", Root, SearchQuery { limit: 3, ..all() }),
        ("limit_past_the_ceiling", Root, SearchQuery { limit: 50_000, ..all() }),
        ("no_filter_at_all", Root, base()),
        ("share_unscoped", Share, named("report")),
        (
            "share_scope_subfolder",
            Share,
            scoped(all(), &["/Volumes/cmdr-golden-share/papers"]),
        ),
        ("share_scope_mount_root", Share, scoped(named("report"), &[SHARE_ROOT])),
        (
            "share_scope_outside_the_mount",
            Share,
            scoped(all(), &["/elsewhere/papers"]),
        ),
        ("share_case_platform_default", Share, named("img_0001.jpg")),
    ]
}

/// The live traces: a run over covered ground, a scoped one with an unresolved
/// path, a capped one, and two that fail.
fn live_cases() -> Vec<(&'static str, Volume, SearchQuery)> {
    use Volume::Root;
    vec![
        ("live_covered", Root, named("report")),
        (
            "live_scope_half_resolved",
            Root,
            scoped(named("*"), &["/cmdr-golden/Docs", "/cmdr-golden/Nope"]),
        ),
        ("live_capped", Root, SearchQuery { limit: 3, ..named("*") }),
        (
            "live_invalid_regex",
            Root,
            SearchQuery {
                pattern_type: PatternType::Regex,
                ..named("[unclosed")
            },
        ),
        (
            "live_folder_size_filter",
            Root,
            SearchQuery {
                min_size: Some(1_000_000),
                ..base()
            },
        ),
    ]
}

/// The `~` cases: a home folder inside the corpus, so a `~` scope expands against
/// it and result rows under it read `~/…`.
fn home_cases() -> Vec<(&'static str, &'static str, SearchQuery)> {
    vec![
        (
            "home_tilde_scope",
            "/cmdr-golden/Projects",
            scoped(named("*.rs"), &["~/cmdr"]),
        ),
        (
            "home_bare_tilde_scope",
            "/cmdr-golden/Projects",
            scoped(named("report"), &["~"]),
        ),
        ("home_parent_paths_unscoped", "/cmdr-golden/Projects", named("report")),
        ("home_only_prefixes_a_folder_name", "/cmdr-golden/Pro", named("report")),
    ]
}

fn target_for(volume: Volume, query: &SearchQuery) -> Target {
    let include_paths = query.include_paths.clone().unwrap_or_default();
    Target {
        volume_id: match volume {
            Volume::Root => ROOT_VOLUME_ID.to_string(),
            Volume::Share => SHARE_VOLUME_ID.to_string(),
        },
        from_scope: !include_paths.is_empty(),
        include_paths,
        can_be_indexed: true,
        // The boot disk reads through `std::fs`; the share is a backend volume. The corpus
        // paths exist nowhere, so `realpath` would keep them literal either way.
        namespace: match volume {
            Volume::Root => crate::search::index_source::PathNamespace::Os,
            Volume::Share => crate::search::index_source::PathNamespace::Backend,
        },
    }
}

/// Where a platform's goldens live.
fn golden_file(platform: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("src/search/execute/tests/goldens/{platform}.json"))
}

/// This build's platform, the one `NameRules::host()` answers for.
fn host_platform() -> &'static str {
    if cfg!(target_os = "macos") { "macos" } else { "linux" }
}

fn read_goldens(platform: &str) -> BTreeMap<String, Value> {
    let text = std::fs::read_to_string(golden_file(platform)).expect("read the goldens");
    serde_json::from_str(&text).expect("the goldens parse")
}

/// Every case's answer under `rules` (through [`golden_run`] for this machine's own
/// rules, [`golden_run_under`] for another's), over freshly written corpora.
fn answers_under(rules: NameRules) -> BTreeMap<String, Value> {
    let dir = tempfile::tempdir().expect("a temp dir for the golden indexes");
    let root = load(dir.path(), "index-root-golden.db", &root_corpus(), None);
    let share = load(dir.path(), "index-share-golden.db", &share_corpus(), Some(SHARE_ROOT));
    let (root_weights, share_weights) = (root_weights(), share_weights());
    let volume = |v: Volume| match v {
        Volume::Root => (&root, &root_weights),
        Volume::Share => (&share, &share_weights),
    };

    let mut actual: BTreeMap<String, Value> = BTreeMap::new();
    for (name, v, query) in cases() {
        let (loaded, weights) = volume(v);
        let target = target_for(v, &query);
        let answer = if rules == NameRules::host() {
            golden_run(&query, &target, loaded, weights)
        } else {
            golden_run_under(rules, None, &query, &target, loaded, weights)
        };
        actual.insert(
            name.to_string(),
            answer.unwrap_or_else(|message| json!({ "error": message })),
        );
    }
    for (name, home, query) in home_cases() {
        let (loaded, weights) = volume(Volume::Root);
        let target = target_for(Volume::Root, &query);
        let answer = golden_run_under(rules, Some(home), &query, &target, loaded, weights);
        actual.insert(
            name.to_string(),
            answer.unwrap_or_else(|message| json!({ "error": message })),
        );
    }
    for (name, v, query) in live_cases() {
        let (loaded, weights) = volume(v);
        actual.insert(
            name.to_string(),
            golden_live_trace(rules, &query, &target_for(v, &query), loaded, weights),
        );
    }
    actual
}

fn assert_same(expected: &BTreeMap<String, Value>, actual: &BTreeMap<String, Value>) {
    let mut moved = Vec::new();
    for name in expected
        .keys()
        .chain(actual.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        if expected.get(name) != actual.get(name) {
            moved.push(format!(
                "{name}:\n  expected {}\n  actual   {}",
                expected.get(name).map_or("<missing>".into(), Value::to_string),
                actual.get(name).map_or("<missing>".into(), Value::to_string),
            ));
        }
    }
    assert!(
        moved.is_empty(),
        "{} golden(s) moved:\n{}",
        moved.len(),
        moved.join("\n")
    );
}

#[test]
fn search_answers_match_the_goldens() {
    let actual = answers_under(NameRules::host());
    if std::env::var_os("CMDR_RECORD_SEARCH_GOLDENS").is_some() {
        let text = serde_json::to_string_pretty(&actual).expect("goldens serialize") + "\n";
        std::fs::write(golden_file(host_platform()), text).expect("write the goldens");
        return;
    }
    assert_same(&read_goldens(host_platform()), &actual);
}

/// The other platform's goldens, answered here under that platform's name rules: the
/// rules are a value, so a Mac's answers are checkable on Linux CI and the other way
/// round. The corpus spells its scope paths exactly as stored, so the host index's own
/// path fold (which the rules don't reach) resolves them the same either way.
#[test]
fn the_other_platforms_goldens_hold_under_its_rules() {
    let (platform, rules) = if NameRules::host() == NameRules::MACOS {
        ("linux", NameRules::LINUX)
    } else {
        ("macos", NameRules::MACOS)
    };
    assert_same(&read_goldens(platform), &answers_under(rules));
}

// ── A source that can't be read ──────────────────────────────────────
//
// The arena is loaded, then the database behind it breaks, so the search's own reads
// (folder sizes, the scope) are what meet the failure. ❌ None of them may answer
// as if the read had worked: an empty size reads as "unknown", an unresolved scope as
// a typo, and both are confident wrong answers.

/// The boot corpus, loaded, with `sql` run against its database afterwards.
fn broken_root(sql: &str) -> (tempfile::TempDir, LoadedVolume) {
    let dir = tempfile::tempdir().expect("a temp dir for the golden index");
    let loaded = load(dir.path(), "index-root-golden.db", &root_corpus(), None);
    let conn = IndexStore::open_write_connection(&dir.path().join("index-root-golden.db")).expect("a write connection");
    conn.execute_batch(sql).expect("break the golden index");
    (dir, loaded)
}

#[test]
fn a_folder_size_that_cant_be_read_fails_the_search() {
    let (_dir, loaded) = broken_root("DROP TABLE dir_stats;");
    let query = SearchQuery {
        is_directory: Some(true),
        ..base()
    };
    let answer = golden_run(&query, &target_for(Volume::Root, &query), &loaded, &root_weights());
    assert!(
        answer.is_err(),
        "folders with no size, answered as if sized: {answer:?}"
    );
}

#[test]
fn a_scope_that_cant_be_resolved_fails_the_search() {
    let (_dir, loaded) = broken_root("ALTER TABLE entries RENAME TO entries_gone;");
    let query = scoped(named("report"), &["/cmdr-golden/Docs"]);
    let answer = golden_run(&query, &target_for(Volume::Root, &query), &loaded, &root_weights());
    assert!(answer.is_err(), "an unreadable scope, answered as a typo: {answer:?}");
}

/// A live run says the index can't be read, in words a person can act on, ❌ never
/// "invalid pattern" over a raw SQL message: the query was fine.
#[test]
fn a_source_read_error_reaches_a_live_run_as_an_unreadable_index() {
    let (_dir, loaded) = broken_root("DROP TABLE dir_stats;");
    let query = SearchQuery {
        min_size: Some(1_000_000),
        ..base()
    };
    let trace = golden_live_trace(
        NameRules::host(),
        &query,
        &target_for(Volume::Root, &query),
        &loaded,
        &root_weights(),
    );
    assert_eq!(
        trace["errors"],
        json!([{
            "runId": "golden-run",
            "error": "indexUnreadable",
            "message": "Cmdr can’t read this drive’s index. Re-indexing the drive fixes it.",
        }])
    );
}
