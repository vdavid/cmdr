//! Recognizing File Provider domains (macOS): domain roots, and whether a path
//! sits inside one.
//!
//! A File Provider domain (Dropbox, Google Drive, iCloud Drive, MacDroid, …) is
//! NOT a mount point: its root reports the same `st_dev` as `$HOME` and never
//! appears in `mount`, so the usual volume-boundary detectors are blind to it.
//! What DOES mark it is an extended attribute, `com.apple.file-provider-domain-id`,
//! present on the domain root only — not on its children, not on
//! `~/Library/CloudStorage` itself, and not on ordinary folders.
//!
//! The syscall itself costs ~5 µs: a plain APFS xattr read, no XPC, no provider
//! process, so it works while the provider is offline and can't hang. It needs no
//! entitlement. (A whole `domain_id_for_dir` call around it measured 13.9 µs on a
//! busy machine, so budget for the read, not for the syscall alone.)
//!
//! Two consumers, two questions:
//!
//! - **"Is this directory a domain root?"** ([`domain_id_for_dir`]) — the index
//!   scanner's exclusion policy, which walks and wants to know when it just
//!   reached the top of someone's sync tree.
//! - **"Is this path inside ANY domain?"** ([`FileProviderDomains`]) — the sync
//!   badge, which wants to skip asking a provider about a file no provider owns.
//!   An ancestor walk over the first question, memoized per directory.
//!
//! **This is a private Apple xattr and an OPTIMIZATION, never a safety guarantee.**
//! It's an implementation detail of `fileproviderd`, undocumented and not
//! contractual. [`FileProviderDomains`] carries the backstop that fact demands: it
//! checks the marker against the domains this machine actually has before it
//! believes a negative, and answers [`DomainMembership::Undetermined`] when it
//! can't vouch for it. Evidence, the false-positive sweep, and the
//! authoritative-but-costly `NSFileProviderManager` alternative:
//! `docs/notes/fileprovider-domain-detection.md` (verified on macOS 26.5.2, build
//! 25F84, 2026-07-20; the marker re-checked against the live Dropbox and iCloud
//! Drive domains on this machine, 2026-08-21).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

/// The extended attribute `fileproviderd` writes on every File Provider domain
/// root. Its value is `<provider extension bundle id>/<domain identifier>`.
const DOMAIN_ID_XATTR: &str = "com.apple.file-provider-domain-id";

/// Home-relative locations where this machine's domain roots would be, used to
/// check that the marker is still the marker. `~/Library/CloudStorage` holds one
/// directory per third-party domain; `~/Library/Mobile Documents` IS iCloud
/// Drive's domain root rather than a parent of it.
///
/// ❌ This is NOT how membership is decided — that's the ancestor walk, which
/// finds a domain wherever a provider registered it. This list only has to name
/// somewhere a domain plausibly IS, so "no marker anywhere" can be told apart
/// from "no providers on this machine".
const CANDIDATE_DOMAIN_LOCATIONS: &[&str] = &["Library/CloudStorage", "Library/Mobile Documents"];

/// How many directories [`FileProviderDomains`] remembers before it drops the lot
/// and starts over. Refilling costs one xattr read per path component, so a reset
/// is cheaper than tracking recency; the cap only exists so a long browsing
/// session can't grow the map without bound.
const MEMO_CAPACITY: usize = 2048;

/// The File Provider domain identifier for `path`, or `None` when `path` isn't a
/// domain root (the overwhelming majority of directories).
///
/// The returned string is the raw xattr value,
/// `<provider extension bundle id>/<domain identifier>` (for example
/// `com.getdropbox.dropbox.fileprovider/c840514d-…`). Callers that only need the
/// yes/no answer can `.is_some()` it.
///
/// Follows symlinks, so a link INTO a domain reports that domain: callers ask
/// about a place, not about a directory entry. A read failure, a missing
/// attribute, and a non-UTF-8 value all collapse to `None`: this is a hint, so an
/// unreadable path is simply "not recognized".
#[must_use]
pub fn domain_id_for_dir(path: &str) -> Option<String> {
    read_domain_id_xattr(path, DOMAIN_ID_XATTR)
}

/// The read itself, with the attribute name injected so tests can exercise it
/// against a name they're allowed to write. macOS refuses `com.apple.*` xattrs to
/// an unentitled process, so a test that builds its fixture with the real constant
/// can never pass; see the tests below.
fn read_domain_id_xattr(path: &str, name: &str) -> Option<String> {
    let raw = xattr::get(path, name).ok()??;
    String::from_utf8(raw).ok()
}

/// Whether a path sits inside somebody's File Provider domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainMembership {
    /// The path is a domain root, or has one as an ancestor. A cloud provider
    /// manages it, so questions about sync state are worth asking.
    Inside,
    /// No ancestor carries the marker, and the marker was vouched for on this
    /// machine. Nothing here is cloud-managed.
    Outside,
    /// No answer: the path couldn't be resolved, or the marker couldn't be
    /// vouched for (macOS may have stopped writing it). Callers must fall back to
    /// whatever they'd do without this module at all — ❌ never treat it as
    /// [`Outside`](Self::Outside).
    Undetermined,
}

/// The two filesystem questions [`FileProviderDomains`] asks, injected so tests
/// can answer them without a real domain (macOS refuses `com.apple.*` xattrs to an
/// unentitled process, so a fixture can't have a real one).
struct Probes {
    /// The domain marker on one directory, `None` when it isn't a domain root.
    domain_id: DomainIdProbe,
    /// Where this machine's domain roots would be, or `None` when that couldn't
    /// be listed at all (which makes the marker un-vouchable rather than absent).
    candidates: CandidateProbe,
}

/// Reads the domain marker off one path.
type DomainIdProbe = Box<dyn Fn(&Path) -> Option<String> + Send + Sync>;

/// Lists where this machine's domain roots would be.
type CandidateProbe = Box<dyn Fn() -> Option<Vec<PathBuf>> + Send + Sync>;

/// Reads the wall clock, injected so a test can step the re-check window.
type Clock = Box<dyn Fn() -> Instant + Send + Sync>;

/// Answers "is this directory inside a File Provider domain?" without asking a
/// provider anything.
///
/// One instance per consumer; it holds a memo, not a connection. Every answer
/// comes from xattr reads on the path's own ancestors, so it works while a
/// provider is offline and can't hang on XPC. It CAN block the same way any
/// `stat` blocks on a dead network mount, so call it from wherever the caller
/// already does its blocking filesystem work.
///
/// ## Bounded staleness
///
/// Both halves expire after `recheck_after`: a directory's verdict, and the check
/// that the marker still works. Signing into iCloud or installing Dropbox turns a
/// whole tree cloud-managed with no directory-listing change to notice, so a
/// permanently frozen answer would be wrong until the next restart.
pub struct FileProviderDomains {
    probes: Probes,
    recheck_after: Duration,
    clock: Clock,
    state: Mutex<State>,
}

// DEFAULT-OK: an empty memo is the absence of any claim about any directory — the
// resolver simply hasn't looked yet, and every read of it re-derives.
#[derive(Default)]
struct State {
    /// Directory → what the ancestor walk concluded, and when. Holds the raw walk
    /// result; whether a negative is BELIEVED is decided per read, off `marker`.
    dirs: HashMap<PathBuf, Decided<bool>>,
    /// Whether the marker is still present on this machine's own domains.
    marker: Option<Decided<bool>>,
}

#[derive(Clone, Copy)]
struct Decided<T> {
    value: T,
    at: Instant,
}

impl FileProviderDomains {
    /// A resolver that re-checks each answer after `recheck_after`.
    #[must_use]
    pub fn new(recheck_after: Duration) -> Self {
        Self::with_probes(
            recheck_after,
            Probes {
                domain_id: Box::new(|path| domain_id_for_dir(&path.to_string_lossy())),
                candidates: Box::new(candidate_domain_locations),
            },
            Box::new(Instant::now),
        )
    }

    fn with_probes(recheck_after: Duration, probes: Probes, clock: Clock) -> Self {
        Self {
            probes,
            recheck_after,
            clock,
            state: Mutex::new(State::default()),
        }
    }

    /// Whether `dir` is inside a File Provider domain.
    ///
    /// Costs one xattr read per path component the first time it sees a
    /// directory, then a hash lookup (measured at 91 ns) until the answer expires.
    /// Every ancestor it walks past is memoized too, so the second directory in a
    /// tree stops at the first remembered one.
    #[must_use]
    pub fn membership_of_dir(&self, dir: &Path) -> DomainMembership {
        let now = (self.clock)();
        let Some(inside) = self.walk_verdict(dir, now) else {
            return DomainMembership::Undetermined;
        };
        if inside {
            // A hit can't be a false one: the marker is only ever missing, never
            // invented, so a positive needs no vouching.
            return DomainMembership::Inside;
        }
        if self.marker_is_trustworthy(now) {
            DomainMembership::Outside
        } else {
            DomainMembership::Undetermined
        }
    }

    /// Whether `path` — a file OR a directory — is inside a File Provider domain.
    ///
    /// Same walk as [`membership_of_dir`](Self::membership_of_dir) on the parent,
    /// plus one unmemoized read on `path` itself when the parent is
    /// [`Outside`](DomainMembership::Outside): a domain root is a directory like
    /// any other and shows up as a row in its parent's listing, so it has to be
    /// able to answer [`Inside`](DomainMembership::Inside) for itself. The leaf
    /// read stays out of the memo because leaves are files, and there are millions
    /// of them.
    #[must_use]
    pub fn membership_of(&self, path: &Path) -> DomainMembership {
        let Some(parent) = path.parent() else {
            return self.membership_of_dir(path);
        };
        match self.membership_of_dir(parent) {
            DomainMembership::Outside if (self.probes.domain_id)(path).is_some() => DomainMembership::Inside,
            verdict => verdict,
        }
    }

    /// The memoized ancestor walk. `Some(true)` = a domain root is at or above
    /// `dir`, `Some(false)` = none is, `None` = `dir` couldn't be resolved.
    fn walk_verdict(&self, dir: &Path, now: Instant) -> Option<bool> {
        if let Some(remembered) = self.remembered(dir, now) {
            return Some(remembered);
        }
        // Resolve symlinks first: with iCloud's "Desktop & Documents Folders" on,
        // `~/Desktop` is a link into the iCloud domain, and walking the link's own
        // ancestors would find nothing and call the whole domain ordinary.
        let real = std::fs::canonicalize(dir).ok()?;

        let mut walked = vec![dir.to_path_buf()];
        let mut cursor = real.as_path();
        let inside = loop {
            if let Some(remembered) = self.remembered(cursor, now) {
                // Everything below it was already checked and carried no marker,
                // so the ancestor's verdict is theirs too.
                break remembered;
            }
            walked.push(cursor.to_path_buf());
            if (self.probes.domain_id)(cursor).is_some() {
                break true;
            }
            match cursor.parent() {
                Some(parent) => cursor = parent,
                None => break false,
            }
        };

        let mut state = self.state.lock_ignore_poison();
        if state.dirs.len().saturating_add(walked.len()) > MEMO_CAPACITY {
            state.dirs.clear();
        }
        for path in walked {
            state.dirs.insert(path, Decided { value: inside, at: now });
        }
        Some(inside)
    }

    fn remembered(&self, dir: &Path, now: Instant) -> Option<bool> {
        let state = self.state.lock_ignore_poison();
        let decided = state.dirs.get(dir)?;
        self.is_fresh(decided, now).then_some(decided.value)
    }

    /// Whether the marker is still the marker. Checked against the domains this
    /// machine actually has: if `~/Library/CloudStorage` holds folders and
    /// `~/Library/Mobile Documents` exists and NONE of them carries the xattr,
    /// then macOS has stopped writing it and every negative this module produces
    /// would be a lie.
    ///
    /// A machine with no providers at all vouches trivially: there's nothing to
    /// mis-report, and the recheck picks up the first provider the user installs.
    fn marker_is_trustworthy(&self, now: Instant) -> bool {
        if let Some(decided) = self.state.lock_ignore_poison().marker
            && self.is_fresh(&decided, now)
        {
            return decided.value;
        }
        let trustworthy = match (self.probes.candidates)() {
            Some(candidates) => {
                candidates.is_empty() || candidates.iter().any(|path| (self.probes.domain_id)(path).is_some())
            }
            None => false,
        };
        self.state.lock_ignore_poison().marker = Some(Decided {
            value: trustworthy,
            at: now,
        });
        trustworthy
    }

    fn is_fresh<T>(&self, decided: &Decided<T>, now: Instant) -> bool {
        now.saturating_duration_since(decided.at) < self.recheck_after
    }

    /// A resolver that treats exactly `roots` as domain roots and vouches for the
    /// marker, so a test can shape a machine's cloud domains without having one.
    /// macOS refuses `com.apple.*` xattrs to an unentitled process, which is why
    /// this can't be done by writing the real marker onto a fixture directory.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_domain_roots(roots: Vec<PathBuf>, recheck_after: Duration) -> Self {
        let marked = roots.clone();
        Self::with_probes(
            recheck_after,
            Probes {
                domain_id: Box::new(move |path| {
                    marked
                        .iter()
                        .any(|root| root == path)
                        .then(|| "com.example.provider/domain".to_string())
                }),
                candidates: Box::new(move || Some(roots.clone())),
            },
            Box::new(Instant::now),
        )
    }
}

/// Where this machine's File Provider domain roots would be: every directory
/// directly inside `~/Library/CloudStorage`, plus `~/Library/Mobile Documents`
/// when it exists. `None` when a location exists but couldn't be listed, which is
/// "can't vouch for the marker" rather than "no domains".
fn candidate_domain_locations() -> Option<Vec<PathBuf>> {
    let home = dirs::home_dir()?;
    let mut candidates = Vec::new();
    for relative in CANDIDATE_DOMAIN_LOCATIONS {
        let location = home.join(relative);
        match std::fs::read_dir(&location) {
            Ok(entries) => {
                // The location itself is a domain root for iCloud Drive and a
                // plain parent for CloudStorage, so both it and its child
                // directories are candidates.
                candidates.push(location);
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        candidates.push(entry.path());
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    Some(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const RECHECK: Duration = Duration::from_secs(600);

    /// An ordinary directory carries no domain-id xattr, and a path that doesn't
    /// exist reads as "not recognized" rather than panicking.
    #[test]
    fn ordinary_directory_is_not_a_domain_root() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().to_string_lossy().into_owned();
        assert_eq!(domain_id_for_dir(&path), None, "a plain temp dir is no domain root");
        assert_eq!(
            domain_id_for_dir(&format!("{path}/nope")),
            None,
            "a missing path is None"
        );
    }

    /// A directory carrying the domain-id xattr reads back as that domain, value
    /// verbatim.
    ///
    /// ❌ Do NOT write `DOMAIN_ID_XATTR` here to build the fixture. macOS refuses
    /// `com.apple.*` extended attributes to an unentitled process with EPERM, so a
    /// test that sets it fails on a real machine no matter what this module does
    /// (verified on macOS 26.5.2, 2026-07-21: `xattr -w com.apple.file-provider-domain-id`
    /// → "Operation not permitted", while a `com.example.*` name succeeds). The
    /// read path is exercised against a name we ARE allowed to write; the constant
    /// itself is covered by `the_domain_id_xattr_name_is_the_one_macos_uses`.
    #[test]
    fn a_directory_carrying_the_xattr_reports_its_value_verbatim() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().to_string_lossy().into_owned();
        let value = "com.example.provider/2f3c1a90-0000-4000-8000-000000000001";
        let writable_name = "com.example.file-provider-domain-id";
        xattr::set(&path, writable_name, value.as_bytes()).expect("set the stand-in xattr");

        assert_eq!(
            read_domain_id_xattr(&path, writable_name),
            Some(value.to_string()),
            "the reader returns the attribute's value unchanged"
        );
    }

    /// The production constant is the name macOS actually uses. Split out because
    /// the fixture above cannot use it (see that test's note), so without this the
    /// name itself would be untested and a typo would silently disable detection.
    #[test]
    fn the_domain_id_xattr_name_is_the_one_macos_uses() {
        assert_eq!(DOMAIN_ID_XATTR, "com.apple.file-provider-domain-id");
    }

    /// A test clock, and the pair of scripted probes that stand in for a machine's
    /// domains.
    struct Machine {
        /// Directories that carry the marker, i.e. the domain roots.
        roots: Arc<Mutex<Vec<PathBuf>>>,
        /// What `candidate_domain_locations` finds on this machine.
        candidates: Arc<Mutex<Option<Vec<PathBuf>>>>,
        /// Every path the marker was read on, so a test can count syscalls.
        reads: Arc<Mutex<Vec<PathBuf>>>,
        now: Arc<Mutex<Instant>>,
    }

    impl Machine {
        fn new() -> Self {
            Self {
                roots: Arc::new(Mutex::new(Vec::new())),
                candidates: Arc::new(Mutex::new(Some(Vec::new()))),
                reads: Arc::new(Mutex::new(Vec::new())),
                now: Arc::new(Mutex::new(Instant::now())),
            }
        }

        fn resolver(&self) -> FileProviderDomains {
            let roots = Arc::clone(&self.roots);
            let reads = Arc::clone(&self.reads);
            let candidates = Arc::clone(&self.candidates);
            let now = Arc::clone(&self.now);
            FileProviderDomains::with_probes(
                RECHECK,
                Probes {
                    domain_id: Box::new(move |path| {
                        reads.lock_ignore_poison().push(path.to_path_buf());
                        roots
                            .lock_ignore_poison()
                            .iter()
                            .any(|root| root == path)
                            .then(|| "com.example.provider/domain".to_string())
                    }),
                    candidates: Box::new(move || candidates.lock_ignore_poison().clone()),
                },
                Box::new(move || *now.lock_ignore_poison()),
            )
        }

        fn add_domain_root(&self, path: &Path) {
            self.roots.lock_ignore_poison().push(path.to_path_buf());
            self.candidates
                .lock_ignore_poison()
                .get_or_insert_with(Vec::new)
                .push(path.to_path_buf());
        }

        /// A machine whose provider locations exist but carry no marker: what
        /// Apple dropping the xattr looks like from here.
        fn with_unmarked_candidate(&self, path: &Path) {
            self.candidates
                .lock_ignore_poison()
                .get_or_insert_with(Vec::new)
                .push(path.to_path_buf());
        }

        fn advance(&self, by: Duration) {
            *self.now.lock_ignore_poison() += by;
        }

        fn reads(&self) -> usize {
            self.reads.lock_ignore_poison().len()
        }
    }

    /// A real directory tree, because the walk canonicalizes before it climbs.
    fn tree(parts: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().expect("temp dir");
        let mut path = root.path().canonicalize().expect("canonical temp dir");
        for part in parts {
            path = path.join(part);
        }
        std::fs::create_dir_all(&path).expect("create the tree");
        (root, path)
    }

    #[test]
    fn a_directory_with_no_domain_above_it_is_outside() {
        let machine = Machine::new();
        let (_root, deep) = tree(&["projects", "cmdr", "src"]);

        assert_eq!(machine.resolver().membership_of_dir(&deep), DomainMembership::Outside);
    }

    /// The ancestor walk is the whole point: the marker sits on the domain ROOT,
    /// and the pane is looking at a folder several levels down inside it.
    #[test]
    fn a_directory_under_a_domain_root_is_inside() {
        let machine = Machine::new();
        let (root, deep) = tree(&["Dropbox", "photos", "2026"]);
        machine.add_domain_root(&root.path().canonicalize().expect("canonical").join("Dropbox"));

        assert_eq!(machine.resolver().membership_of_dir(&deep), DomainMembership::Inside);
    }

    /// iCloud Drive's shape, which a path-prefix heuristic gets wrong: the domain
    /// root is `~/Library/Mobile Documents` itself, and `com~apple~CloudDocs` is a
    /// plain child inside the domain rather than the root.
    #[test]
    fn icloud_drives_domain_root_is_mobile_documents_not_its_clouddocs_child() {
        let machine = Machine::new();
        let (root, deep) = tree(&["Library", "Mobile Documents", "com~apple~CloudDocs", "Notes"]);
        let mobile_documents = root
            .path()
            .canonicalize()
            .expect("canonical")
            .join("Library")
            .join("Mobile Documents");
        machine.add_domain_root(&mobile_documents);

        let resolver = machine.resolver();
        assert_eq!(
            resolver.membership_of_dir(&deep),
            DomainMembership::Inside,
            "a folder under com~apple~CloudDocs is in the domain"
        );
        assert_eq!(
            resolver.membership_of_dir(&mobile_documents),
            DomainMembership::Inside,
            "the domain root itself is in the domain"
        );
    }

    /// A domain root is a row in its parent's listing, and its parent is an
    /// ordinary folder. Asking about the ROW has to find the domain the row is.
    #[test]
    fn a_domain_root_is_inside_its_own_domain_even_though_its_parent_is_not() {
        let machine = Machine::new();
        let (root, dropbox) = tree(&["CloudStorage", "Dropbox"]);
        machine.add_domain_root(&dropbox);
        let cloud_storage = root.path().canonicalize().expect("canonical").join("CloudStorage");
        let resolver = machine.resolver();

        assert_eq!(
            resolver.membership_of(&dropbox),
            DomainMembership::Inside,
            "the domain root itself"
        );
        assert_eq!(
            resolver.membership_of(&cloud_storage),
            DomainMembership::Outside,
            "the plain folder holding it"
        );
        assert_eq!(
            resolver.membership_of(&dropbox.join("photo.jpg")),
            DomainMembership::Inside,
            "a file inside the domain"
        );
    }

    /// The memo is what makes this cheaper than the probe it replaces: a second
    /// directory in the same tree costs no reads at all beyond its own.
    #[test]
    fn the_walk_is_memoized_across_siblings() {
        let machine = Machine::new();
        let (_root, deep) = tree(&["projects", "cmdr", "src"]);
        let resolver = machine.resolver();

        let _ = resolver.membership_of_dir(&deep);
        let after_first = machine.reads();
        assert!(after_first > 3, "the first walk climbed to the filesystem root");

        let _ = resolver.membership_of_dir(&deep);
        assert_eq!(machine.reads(), after_first, "a repeat directory reads nothing");

        let sibling = deep.parent().expect("parent").join("tests");
        std::fs::create_dir_all(&sibling).expect("create the sibling");
        let _ = resolver.membership_of_dir(&sibling);
        assert_eq!(
            machine.reads(),
            after_first + 1,
            "the sibling checked itself and stopped at the remembered parent"
        );
    }

    /// Installing Dropbox or signing into iCloud turns a whole tree cloud-managed
    /// with no directory-listing change, so nothing invalidates this. The bounded
    /// re-check is the only thing that catches it.
    #[test]
    fn a_provider_installed_later_is_noticed_once_the_answer_expires() {
        let machine = Machine::new();
        let (root, deep) = tree(&["CloudStorage", "Dropbox", "photos"]);
        let resolver = machine.resolver();
        assert_eq!(resolver.membership_of_dir(&deep), DomainMembership::Outside);

        machine.add_domain_root(
            &root
                .path()
                .canonicalize()
                .expect("canonical")
                .join("CloudStorage")
                .join("Dropbox"),
        );
        assert_eq!(
            resolver.membership_of_dir(&deep),
            DomainMembership::Outside,
            "inside the window the remembered answer stands"
        );

        machine.advance(RECHECK + Duration::from_secs(1));
        assert_eq!(
            resolver.membership_of_dir(&deep),
            DomainMembership::Inside,
            "the re-check found the new domain"
        );
    }

    /// The backstop. If macOS ever stops writing the marker, every negative here
    /// becomes a lie — so a machine that HAS provider folders and shows no marker
    /// on any of them gets no negatives at all.
    #[test]
    fn a_machine_whose_domains_lost_the_marker_gets_no_negatives() {
        let machine = Machine::new();
        let (root, deep) = tree(&["projects", "cmdr"]);
        machine.with_unmarked_candidate(&root.path().canonicalize().expect("canonical").join("projects"));

        assert_eq!(
            machine.resolver().membership_of_dir(&deep),
            DomainMembership::Undetermined,
            "the marker couldn't be vouched for, so nothing is called ordinary"
        );
    }

    /// A machine with no cloud providers at all still gets the fast path: there's
    /// no domain to mis-report, and the re-check catches the first one installed.
    #[test]
    fn a_machine_with_no_providers_still_trusts_the_marker() {
        let machine = Machine::new();
        let (_root, deep) = tree(&["projects"]);

        assert_eq!(machine.resolver().membership_of_dir(&deep), DomainMembership::Outside);
    }

    /// Provider locations that exist but can't be listed are not evidence of
    /// absence, so they suspend the fast path rather than vouching for it.
    #[test]
    fn unlistable_provider_locations_suspend_the_fast_path() {
        let machine = Machine::new();
        *machine.candidates.lock_ignore_poison() = None;
        let (_root, deep) = tree(&["projects"]);

        assert_eq!(
            machine.resolver().membership_of_dir(&deep),
            DomainMembership::Undetermined
        );
    }

    /// A path that isn't there can't be resolved, and a guess would be the wrong
    /// kind of answer.
    #[test]
    fn a_path_that_does_not_exist_is_undetermined() {
        let machine = Machine::new();
        let (root, _deep) = tree(&["projects"]);

        assert_eq!(
            machine.resolver().membership_of_dir(&root.path().join("gone")),
            DomainMembership::Undetermined
        );
    }

    /// The memo is bounded, so a long browsing session can't grow it forever.
    #[test]
    fn the_memo_is_bounded() {
        let machine = Machine::new();
        let (root, _deep) = tree(&["dirs"]);
        let resolver = machine.resolver();

        for n in 0..(MEMO_CAPACITY + 50) {
            let dir = root.path().join("dirs").join(format!("d{n}"));
            std::fs::create_dir_all(&dir).expect("create");
            let _ = resolver.membership_of_dir(&dir);
        }

        assert!(
            resolver.state.lock_ignore_poison().dirs.len() <= MEMO_CAPACITY,
            "the memo stayed inside its cap"
        );
    }

    /// The production candidate list names real locations and doesn't panic on a
    /// machine that has none of them.
    #[test]
    fn the_production_candidate_list_reads_this_machine() {
        let candidates = candidate_domain_locations();
        if let Some(candidates) = candidates {
            for candidate in candidates {
                assert!(candidate.is_absolute(), "{} is absolute", candidate.display());
            }
        }
    }
}
