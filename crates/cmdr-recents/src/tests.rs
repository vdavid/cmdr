use std::cell::Cell;

use super::*;

/// A minimal entry: dedupes on `key`, which is all the list rules look at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: String,
    key: String,
}

impl RecentItem for Entry {
    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    fn dedupe_key(&self) -> String {
        self.key.clone()
    }
}

thread_local! {
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
}

/// An id no other test entry has.
fn next_id() -> String {
    NEXT_ID.with(|next| {
        next.set(next.get() + 1);
        format!("id-{}", next.get())
    })
}

fn entry(key: &str) -> Entry {
    Entry {
        id: next_id(),
        key: key.to_string(),
    }
}

fn keys(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.key.as_str()).collect()
}

fn add(entries: &mut Vec<Entry>, entry: Entry, max_count: usize) {
    add_to(entries, entry, max_count, next_id);
}

#[test]
fn add_puts_the_newest_entry_on_top() {
    let mut entries = Vec::new();
    add(&mut entries, entry("a"), 10);
    add(&mut entries, entry("b"), 10);
    assert_eq!(keys(&entries), ["b", "a"]);
}

#[test]
fn add_dedupes_by_key_and_the_newest_copy_wins() {
    let mut entries = Vec::new();
    add(&mut entries, entry("a"), 10);
    add(&mut entries, entry("b"), 10);
    let again = entry("a");
    let again_id = again.id.clone();
    add(&mut entries, again, 10);

    assert_eq!(keys(&entries), ["a", "b"]);
    assert_eq!(entries[0].id, again_id);
}

#[test]
fn add_collapses_every_earlier_copy_of_a_key() {
    let mut entries = vec![entry("a"), entry("a"), entry("b")];
    add(&mut entries, entry("a"), 10);
    assert_eq!(keys(&entries), ["a", "b"]);
}

#[test]
fn add_enforces_the_cap_by_dropping_the_oldest() {
    let mut entries = Vec::new();
    for i in 0..5 {
        add(&mut entries, entry(&format!("q{i}")), 3);
    }
    assert_eq!(keys(&entries), ["q4", "q3", "q2"]);
}

#[test]
fn add_with_a_zero_cap_empties_the_list() {
    let mut entries = vec![entry("a")];
    add(&mut entries, entry("b"), 0);
    assert!(entries.is_empty());
}

#[test]
fn add_mints_an_id_only_when_the_callers_collides() {
    let mut entries = Vec::new();
    let mut minted = 0;
    let fixed = |key: &str| Entry {
        id: "fixed-id".into(),
        key: key.into(),
    };
    add_to(&mut entries, fixed("a"), 10, || {
        minted += 1;
        "unused".into()
    });
    assert_eq!(minted, 0, "a free id is kept");
    add_to(&mut entries, fixed("b"), 10, || "minted".into());
    assert_eq!(
        entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["minted", "fixed-id"]
    );
}

#[test]
fn trim_drops_the_oldest_and_says_so() {
    let mut entries = vec![entry("c"), entry("b"), entry("a")];
    assert!(trim_to(&mut entries, 2));
    assert_eq!(keys(&entries), ["c", "b"]);
    assert!(!trim_to(&mut entries, 10), "a list under the cap is left alone");
    assert!(trim_to(&mut entries, 0));
    assert!(entries.is_empty());
}

#[test]
fn remove_says_whether_it_found_anything() {
    let mut entries = vec![entry("a")];
    let id = entries[0].id.clone();
    assert!(remove_from(&mut entries, &id));
    assert!(entries.is_empty());
    assert!(!remove_from(&mut entries, &id));
}

#[test]
fn a_rewrite_that_made_two_rows_one_keeps_the_newer() {
    let mut entries = vec![entry("a"), entry("b"), entry("a")];
    let newest = entries[0].id.clone();
    assert!(dedupe_keeping_newest(&mut entries));
    assert_eq!(keys(&entries), ["a", "b"]);
    assert_eq!(entries[0].id, newest);
    assert!(!dedupe_keeping_newest(&mut entries));
}
