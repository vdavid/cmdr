//! The two sources answer alike: the goldens' corpus, read through the drive index
//! (`IndexSource`, the desktop) and held in memory (`cmdr_search::MemorySource`, the
//! browser demo), gives the same answer to every golden query, `entry_id`s, tie
//! order, folder sizes, and size sorts included. That's what lets the demo say it
//! runs the real search: one core, rows from another source, proven identical.
//!
//! The index side enters through [`golden_run_under`], the goldens' own door, so this
//! compares against exactly what the goldens pin. The folder sizes in memory are the
//! corpus's own `dir_stats` rows ([`folder_sizes`]), keyed by the memory source's ids.

use std::collections::{BTreeMap, HashMap};

use cmdr_search::testing::FolderSizes;
use cmdr_search::{MemoryRow, MemorySource, SearchRows};
use serde_json::{Value, json};

use super::goldens::{
    Row, SHARE_ROOT, Volume, assert_same, cases, folder_sizes, golden_run_under, half_json, home_cases, load, named,
    root_corpus, root_weights, scoped, share_corpus, share_weights, target_for,
};
use super::*;
use crate::search::context::NameRules;
use crate::search::index_source::scope_pairs;

/// A corpus held in memory under `rules`, with its folder sizes.
fn in_memory(rows: &[Row], path_prefix: &str, rules: NameRules) -> (MemorySource, FolderSizes) {
    let memory_rows = rows
        .iter()
        .map(|row| MemoryRow {
            path: row.path.to_string(),
            is_directory: row.is_directory,
            size: row.size,
            modified_at: row.modified_at,
        })
        .collect();
    let source = MemorySource::new(memory_rows, path_prefix, rules).expect("the corpus is a tree");
    let sizes = folder_sizes(rows)
        .into_iter()
        .map(|(path, _, bytes)| (source.id_of(path).expect("every folder is in memory"), bytes))
        .collect::<HashMap<_, _>>();
    (source, FolderSizes(sizes))
}

/// [`golden_run_under`]'s answer, over memory: the same scope pairs, context, and
/// result shape.
fn memory_run(
    rules: NameRules,
    home_dir: Option<&str>,
    query: &SearchQuery,
    target: &Target,
    (source, sizes): &(MemorySource, FolderSizes),
    weights: &ImportanceWeights,
) -> Result<Value, String> {
    let ctx = SearchContext {
        rules,
        home_dir,
        weights,
    };
    let scope = scope_pairs(&target.include_paths, home_dir, target.namespace);
    search_covered(source, sizes, query, &scope, &ctx)
        .map(|half| half_json(&half))
        .map_err(|e| e.to_string())
}

/// Scopes spelled another way than the corpus stores them. Only this machine's rules
/// can be checked on these: the index folds a scope with the fold it was written
/// under (the host's), while memory folds with the rules it's given, so under the
/// other platform's rules the two read different drives, by design.
fn respelled_scope_cases() -> Vec<(&'static str, SearchQuery)> {
    vec![
        ("scope_in_another_case", scoped(named("report"), &["/cmdr-golden/docs"])),
        (
            "scope_nfc_folder_typed_nfd",
            scoped(named("*"), &["/cmdr-golden/Photos/Cafe\u{301}"]),
        ),
        (
            "scope_nfd_name_typed_nfc",
            scoped(named("*"), &["/cmdr-golden/Docs/r\u{e9}sum\u{e9}.txt"]),
        ),
    ]
}

/// Every case's answer from both sources, under `rules`.
fn both_answers(rules: NameRules) -> (BTreeMap<String, Value>, BTreeMap<String, Value>) {
    let dir = tempfile::tempdir().expect("a temp dir for the golden indexes");
    let root = load(dir.path(), "index-root-golden.db", &root_corpus(), None);
    let share = load(dir.path(), "index-share-golden.db", &share_corpus(), Some(SHARE_ROOT));
    let root_memory = in_memory(&root_corpus(), "", rules);
    let share_memory = in_memory(&share_corpus(), SHARE_ROOT, rules);
    let (root_weights, share_weights) = (root_weights(), share_weights());

    let mut queries: Vec<(String, Volume, Option<&str>, SearchQuery)> = Vec::new();
    for (name, volume, query) in cases() {
        queries.push((name.to_string(), volume, None, query));
    }
    for (name, home, query) in home_cases() {
        queries.push((name.to_string(), Volume::Root, Some(home), query));
    }
    if rules == NameRules::host() {
        for (name, query) in respelled_scope_cases() {
            queries.push((name.to_string(), Volume::Root, None, query));
        }
    }

    let (mut from_index, mut from_memory) = (BTreeMap::new(), BTreeMap::new());
    for (name, volume, home, query) in queries {
        let (loaded, memory, weights) = match volume {
            Volume::Root => (&root, &root_memory, &root_weights),
            Volume::Share => (&share, &share_memory, &share_weights),
        };
        let target = target_for(volume, &query);
        let as_json = |answer: Result<Value, String>| answer.unwrap_or_else(|message| json!({ "error": message }));
        from_index.insert(
            name.clone(),
            as_json(golden_run_under(rules, home, &query, &target, loaded, weights)),
        );
        from_memory.insert(name, as_json(memory_run(rules, home, &query, &target, memory, weights)));
    }
    (from_index, from_memory)
}

#[test]
fn memory_holds_the_same_arena_the_index_loads() {
    let dir = tempfile::tempdir().expect("a temp dir for the golden index");
    for (rows, prefix) in [(root_corpus(), None), (share_corpus(), Some(SHARE_ROOT))] {
        let loaded = load(dir.path(), "index-golden.db", &rows, prefix);
        let (memory, _) = in_memory(&rows, prefix.unwrap_or(""), NameRules::host());
        let row_of = |arena: &SearchIndex, i: usize| {
            (
                arena.id(i),
                arena.parent_id(i),
                arena.name(i).to_string(),
                arena.is_directory(i),
                arena.size(i),
                arena.modified_at(i),
            )
        };
        let (index, held) = (&*loaded.index, memory.arena());
        let index_rows: Vec<_> = (0..index.len()).map(|i| row_of(index, i)).collect();
        let memory_rows: Vec<_> = (0..held.len()).map(|i| row_of(held, i)).collect();
        assert_eq!(index_rows, memory_rows);
        std::fs::remove_file(dir.path().join("index-golden.db")).expect("clear the index for the next corpus");
    }
}

#[test]
fn memory_answers_every_golden_query_as_the_index_does_under_the_mac_rules() {
    let (from_index, from_memory) = both_answers(NameRules::MACOS);
    assert_same(&from_index, &from_memory);
}

#[test]
fn memory_answers_every_golden_query_as_the_index_does_under_the_linux_rules() {
    let (from_index, from_memory) = both_answers(NameRules::LINUX);
    assert_same(&from_index, &from_memory);
}
