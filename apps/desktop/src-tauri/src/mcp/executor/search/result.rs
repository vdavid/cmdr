//! The typed result both search tools answer with.
//!
//! One shape, two consumers (an external MCP client and Ask Cmdr), because the
//! alternative — a formatted text table — reported coverage as English sentences
//! the caller had to parse, carried no `returned` / `truncated`, and grew without
//! a ceiling (`limit: 5000` serialized to six figures of estimated tokens and
//! pushed the rest of the turn out of the prompt).
//!
//! Everything here is PURE: it folds a [`LiveAnswer`] into a DTO, so the shaping
//! is unit-tested without a Tauri harness or a running search.
//!
//! ## What the caller reads before saying "that's all of them"
//!
//! [`SearchCoverage::complete`], and only that. It is the seven-way conjunction
//! the model would otherwise have to get right every time, and the seven fields
//! stay beside it because each one is a different sentence to the user.

use serde::Serialize;

use crate::pluralize::grouped;
use crate::search::{
    AnswerEnding, LiveAnswer, SearchResultEntry, WalkEnding, format_size, format_timestamp, live::SearchRunCoverage,
};

/// One match, with the raw number beside the spoken one: the raw for filtering
/// and arithmetic, the spoken for the reply. Both come off the ONE formatter pair
/// the search dialog uses, so a model never renders a size Cmdr wouldn't.
///
/// ❌ No `iconId`: a model can't render an icon, and it's a field per row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchHit {
    pub name: String,
    /// The absolute path, ready to hand to another tool.
    pub path: String,
    /// The folder it sits in, as the index stores it (`~/Documents`, not the
    /// expanded home).
    pub parent_path: String,
    pub is_directory: bool,
    /// Absent when the index has no size for the row — a NULL logical size is a
    /// hardlink-deduped row, ❌ never a zero-byte file, so it stays absent
    /// rather than becoming `0`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>,
    /// Last-CHANGED time as a Unix timestamp. Cmdr never records when a file was
    /// saved or opened, so ❌ don't relay this as either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_human: Option<String>,
}

/// What the run could and couldn't speak for. Every field is TYPED, ❌ never a
/// sentence the caller has to parse; the sentences live in
/// [`SearchResult::notes`] beside them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchCoverage {
    /// The one field to read before saying "that's all of them": true only when
    /// the run settled, the walk finished (or had nothing to walk), and every
    /// unreachable-ground field below is clear.
    ///
    /// Two fields deliberately DON'T clear it, because neither means Cmdr failed
    /// to cover its ground: [`capped`](Self::capped) (the rows stopped, the count
    /// didn't) and [`hidden_by_excludes`](Self::hidden_by_excludes) (the caller's
    /// own filter). Both still make the count a floor rather than a total, which
    /// is why [`SearchResult::match_count_human`] wears its `≥` for the first and
    /// why the second always gets a note.
    pub complete: bool,
    /// The wait ran out with the walk still going: the list and the count are a
    /// lower bound, and running the same search again picks up from here.
    /// ❗ Never "no matches".
    pub still_walking: bool,
    /// Directories the walk turned up. Zero means the index answered it all.
    pub folders_found: u64,
    /// The row cap was reached, so no more rows arrive. The walk carried on past
    /// it, so `matchCount` keeps counting.
    pub capped: bool,
    /// How many matches an exclusion rule kept OUT of `matchCount`: the
    /// system/cache/build tier plus any `!` excludes in the scope.
    pub hidden_by_excludes: u32,
    /// Folders the OS refused. The half somebody can act on.
    pub permission_denied: Vec<String>,
    /// Folders Cmdr won't read at all (snapshot trees). Nothing to fix, so
    /// explain rather than offer.
    pub declined: Vec<String>,
    /// Ground another walk holds right now: those results arrive later, ❌ they
    /// are not lost.
    pub still_covering: Vec<String>,
    /// Scope paths Cmdr can't speak for. ❌ Never "that folder doesn't exist" —
    /// it can't tell a typo from a folder nothing has walked.
    pub unresolved_scopes: Vec<String>,
    /// The walk gave up on ground that stopped responding. True makes the list a
    /// lower bound even when the walk otherwise completed.
    pub abandoned_ground: bool,
    /// How many PLACES were given up on, ❌ never the raw folder count: a wedged
    /// mount marks thousands of directories and reads as a broken drive.
    pub abandoned_locations: u32,
}

/// A search answer, whole.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchResult {
    /// The ONE volume this run covered. A search never fans out, so an answer
    /// speaks for this drive and no other.
    pub target_volume_id: String,
    /// Every match the run counted, INCLUDING the ones past the row cap and past
    /// the size cut.
    ///
    /// ❗ Not the `total` the paged tools report: there, `total` is what the page
    /// was cut from, so `returned == total` means "you saw everything". Here the
    /// engine stops emitting rows at the row cap while the count keeps rising, so
    /// `matchCount` can exceed `returned` by orders of magnitude with nothing
    /// wrong. [`coverage.capped`](SearchCoverage::capped) says which.
    pub match_count: u32,
    /// The count as a sentence, wearing `≥` whenever it's a floor rather than a
    /// total. The uncertainty rides INSIDE the string on purpose: a flag in a
    /// sibling field is a flag the model sheds the moment it restates the number.
    pub match_count_human: String,
    /// How many rows `entries` carries.
    pub returned: usize,
    /// The rows were cut to fit one tool result. Narrow the search (a tighter
    /// pattern, a smaller scope, a date range) for the rest — there is no
    /// `offset`, because ranking is top-k and an offset over a re-ranked run
    /// would skip and double-count.
    pub truncated: bool,
    pub entries: Vec<SearchHit>,
    pub coverage: SearchCoverage,
    /// The authored sentences for whatever coverage above is not clear, including
    /// the one genuinely actionable line no flag can replace ("granting Cmdr Full
    /// Disk Access … opens them"). Empty when the run covered everything.
    pub notes: Vec<String>,
}

/// An `ai_search` answer: the same result, plus what the translator made of the
/// caller's prose. The interpretation leads, so a model that got a query it
/// didn't mean can see that before it reads the rows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSearchResult {
    pub interpreted_query: String,
    #[serde(flatten)]
    pub search: SearchResult,
}

/// Fold a finished (or still-walking) run into the result both tools answer with.
///
/// `system_dirs_excluded` is the query's effective setting, which only the notes
/// need: with the default tier already off, advising the caller to turn it off is
/// noise.
pub(crate) fn shape_answer(answer: LiveAnswer, system_dirs_excluded: bool) -> SearchResult {
    let notes = coverage_notes(&answer, system_dirs_excluded);
    let coverage = shape_coverage(&answer);
    let match_count_human = match_count_human(answer.match_count, !coverage.complete || coverage.capped);
    // The row cap can't bound a payload: 200 rows of long paths still outgrow
    // what one tool result may carry, and an oversized result pushes the rest of
    // the caller's turn out of the prompt. ❌ Never a silent cut.
    let fitted = crate::mcp::fit_to_result_budget(answer.entries.into_iter().map(to_hit).collect::<Vec<_>>());
    SearchResult {
        target_volume_id: answer.target_volume_id,
        match_count: answer.match_count,
        match_count_human,
        returned: fitted.items.len(),
        truncated: fitted.truncated,
        entries: fitted.items,
        coverage,
        notes,
    }
}

/// The count as a sentence. `lower_bound` is what puts the `≥` on it.
fn match_count_human(count: u32, lower_bound: bool) -> String {
    let noun = if count == 1 { "match" } else { "matches" };
    let floor = if lower_bound { "≥ " } else { "" };
    format!("{floor}{} {noun}", grouped(u64::from(count)))
}

fn to_hit(entry: SearchResultEntry) -> SearchHit {
    SearchHit {
        name: entry.name,
        path: entry.path,
        parent_path: entry.parent_path,
        is_directory: entry.is_directory,
        size_bytes: entry.size,
        size_human: entry.size.map(format_size),
        modified: entry.modified_at,
        modified_human: entry.modified_at.map(format_timestamp),
    }
}

/// The typed coverage, with `complete` derived once so no caller has to.
fn shape_coverage(answer: &LiveAnswer) -> SearchCoverage {
    let settled: Option<&SearchRunCoverage> = match &answer.ending {
        AnswerEnding::Settled(coverage) => Some(coverage),
        _ => None,
    };
    // A run that never settled hasn't finished covering anything, so the walk
    // reads as unfinished and every list stays empty rather than inventing one.
    let walk_finished = settled.is_some_and(|c| matches!(c.walk, WalkEnding::Completed | WalkEnding::NothingToWalk));
    let permission_denied = settled.map(|c| c.permission_denied.clone()).unwrap_or_default();
    let declined = settled.map(|c| c.declined.clone()).unwrap_or_default();
    let still_covering = settled.map(|c| c.still_covering.clone()).unwrap_or_default();
    let unresolved_scopes = settled.map(|c| c.unresolved_scopes.clone()).unwrap_or_default();
    let abandoned_ground = settled.is_some_and(|c| c.abandoned_ground);
    SearchCoverage {
        complete: walk_finished
            && permission_denied.is_empty()
            && declined.is_empty()
            && still_covering.is_empty()
            && unresolved_scopes.is_empty()
            && !abandoned_ground,
        still_walking: matches!(answer.ending, AnswerEnding::StillWalking),
        folders_found: answer.dirs_found,
        capped: settled.is_some_and(|c| c.capped),
        hidden_by_excludes: settled.map_or(0, |c| c.hidden_by_excludes),
        permission_denied,
        declined,
        still_covering,
        unresolved_scopes,
        abandoned_ground,
        abandoned_locations: settled.map_or(0, |c| c.abandoned_locations),
    }
}

/// Everything the run couldn't answer for, one sentence per cause.
///
/// Every one of these comes off a TYPED field, never a message match. They're the
/// reason an agent can't read an empty list as "there's nothing there": each line
/// says which ground the answer doesn't speak for, and what would open it.
fn coverage_notes(answer: &LiveAnswer, system_dirs_excluded: bool) -> Vec<String> {
    let mut notes = Vec::new();

    if let AnswerEnding::Settled(coverage) = &answer.ending {
        if !coverage.unresolved_scopes.is_empty() {
            notes.push(format!(
                "Note: Cmdr couldn't resolve {}: a typo, or a folder nothing has walked yet.",
                coverage.unresolved_scopes.join(", ")
            ));
        }
        if !coverage.permission_denied.is_empty() {
            notes.push(refusal_note(&coverage.permission_denied, full_disk_access_would_help()));
        }
        if !coverage.declined.is_empty() {
            notes.push(format!(
                "Note: Cmdr never reads snapshot folders (each one is a hardlinked copy of the whole share), so it skipped {}.",
                coverage.declined.join(", ")
            ));
        }
        if !coverage.still_covering.is_empty() {
            notes.push(format!(
                "Note: another search is already walking {}, so this run left it alone. Those results land in the index; run this search again to pick them up.",
                coverage.still_covering.join(", ")
            ));
        }
        if coverage.hidden_by_excludes > 0 {
            // The count is filtered and `complete` doesn't say so — the caller's
            // own filter isn't ground Cmdr failed to cover. It's the difference
            // between "27 files match" and "27, plus 400 inside caches", and for
            // a disk-space question the hidden ones ARE usually the answer.
            // The advice only fits while the default tier is still on: with it
            // already off, everything hidden came from the caller's own `!` excludes.
            let (matches, are) = if coverage.hidden_by_excludes == 1 {
                ("match", "is")
            } else {
                ("matches", "are")
            };
            // Spoken like every other count Cmdr states: a node_modules-heavy drive
            // hides six figures of matches, and `132504` is a number a reader
            // misreads by an order of magnitude.
            let hidden = grouped(u64::from(coverage.hidden_by_excludes));
            notes.push(if system_dirs_excluded {
                format!(
                    "Note: {hidden} more {matches} {are} inside excluded folders and NOT in the count above: the system, cache, and build tier (node_modules, .git, Caches, …) that's hidden by default, plus any ! excludes in the scope. Pass excludeSystemDirs: false to include the default tier. Do that when you're asking where disk space is going, because those folders are usually the answer."
                )
            } else {
                format!("Note: {hidden} more {matches} {are} inside the ! excludes in the scope, and NOT in the count above.")
            });
        }
        if coverage.abandoned_ground {
            // The count is places, not folders: a mount that went to sleep marks
            // every directory the walk had reached inside it, and an agent reading
            // "1,497 folders" would conclude the drive is broken.
            notes.push(if coverage.abandoned_locations > 0 {
                format!(
                    "Note: the walk gave up on {} that stopped responding, so this list is a lower bound. Cmdr retries them on its own.",
                    crate::pluralize::pluralize(u64::from(coverage.abandoned_locations), "place"),
                )
            } else {
                "Note: the walk gave up on folders that stopped responding, so this list is a lower bound. Cmdr retries them on its own.".to_string()
            });
        }
        match coverage.walk {
            WalkEnding::Interrupted => notes.push(
                "Note: the walk stopped before covering everything (the drive went away, or a folder wouldn't read), so this list is a lower bound. Running the search again picks up the rest."
                    .to_string(),
            ),
            WalkEnding::Cancelled => notes.push(
                "Note: the search was stopped before it finished, so this list is a lower bound.".to_string(),
            ),
            // Nothing to say: the index covered the whole scope, or the walk
            // covered everything it took. `foldersFound` still reports the work.
            WalkEnding::NothingToWalk | WalkEnding::Completed => {}
        }
        if answer.dirs_found > 0 && coverage.walk == WalkEnding::Completed {
            notes.push(format!(
                "Cmdr walked {} folders it hadn't indexed yet, so the next search over them is instant.",
                grouped(answer.dirs_found)
            ));
        }
    }

    if matches!(answer.ending, AnswerEnding::StillWalking) {
        notes.push(format!(
            "Note: Cmdr is still walking {} ({} folders so far), so this list and count are a lower bound. The walk keeps filling the index, so running this search again picks up where it left off, or pass a bigger maxWaitSeconds to wait it out.",
            answer.target_volume_id,
            grouped(answer.dirs_found)
        ));
    }

    notes
}

/// Whether pointing at Full Disk Access would actually open a refused folder:
/// on macOS, and only while Cmdr doesn't already hold it. With FDA granted the
/// refusal is something else (a locked folder), and the advice would do nothing.
/// The dialog gates its offer on the same conditions
/// (`coverage-note.ts::offersFullDiskAccess`).
///
/// ⚠️ The platform half is a `#[cfg]`, ❌ never `cfg!(target_os = "macos") && …`:
/// `cfg!` is a runtime value, so the `crate::permissions` call still has to COMPILE
/// on Linux, where that module doesn't exist. It didn't.
#[cfg(target_os = "macos")]
fn full_disk_access_would_help() -> bool {
    !crate::permissions::check_full_disk_access_quiet()
}

/// No such permission exists off macOS: a refusal there is ordinary file
/// permissions, which Cmdr can't grant itself either.
#[cfg(not(target_os = "macos"))]
fn full_disk_access_would_help() -> bool {
    false
}

/// The line for folders the OS refused a walk: the only half of the unreadable
/// ground somebody can act on, so it offers the fix when there is one.
fn refusal_note(paths: &[String], offer_full_disk_access: bool) -> String {
    let refused = format!("Note: the OS refused to let Cmdr read {}.", paths.join(", "));
    if offer_full_disk_access {
        return format!(
            "{refused} Granting Cmdr Full Disk Access in System Settings > Privacy & Security opens them, and the next search covers them."
        );
    }
    refused
}

#[cfg(test)]
mod tests;
