//! Engine tests for the guarded walker. A mock [`ReadDirFn`] serves an in-memory
//! tree and can block on chosen paths, so hang tolerance, honest-skip, and
//! parallel correctness are tested without a real hung mount.

use super::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicI64;

// ── Mock filesystem + reader ─────────────────────────────────────────

/// An in-memory directory tree. `dirs` maps a directory to its children
/// (name + type); `hang` is the set of paths whose read blocks for `hang_dur`.
struct MockFs {
    dirs: HashMap<PathBuf, Vec<(String, RawFileType)>>,
    hang: HashSet<PathBuf>,
    hang_dur: Duration,
}

impl MockFs {
    fn reader(self: Arc<Self>) -> ReadDirFn {
        Arc::new(move |p: &Path| {
            if self.hang.contains(p) {
                std::thread::sleep(self.hang_dur);
            }
            match self.dirs.get(p) {
                Some(children) => Ok(children
                    .iter()
                    .map(|(name, ft)| RawDirEntry {
                        path: p.join(name),
                        file_type: *ft,
                        stat: None,
                    })
                    .collect()),
                None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such mock dir")),
            }
        })
    }
}

/// Builder for a consistent mock tree (every `Dir` child is itself present).
#[derive(Default)]
struct TreeBuilder {
    dirs: HashMap<PathBuf, Vec<(String, RawFileType)>>,
}

impl TreeBuilder {
    fn dir(&mut self, path: &str, children: &[(&str, RawFileType)]) -> &mut Self {
        self.dirs.insert(
            PathBuf::from(path),
            children.iter().map(|(n, t)| ((*n).to_string(), *t)).collect(),
        );
        self
    }

    fn build(&self, hang: HashSet<PathBuf>, hang_dur: Duration) -> Arc<MockFs> {
        Arc::new(MockFs {
            dirs: self.dirs.clone(),
            hang,
            hang_dur,
        })
    }
}

/// Serial reference walk over the same mock tree: the ground truth the parallel
/// engine must match. Returns (dirs whose read succeeds, parent→child path edges).
fn reference_walk(fs: &MockFs, root: &Path) -> (BTreeSet<PathBuf>, BTreeSet<(PathBuf, PathBuf)>) {
    let mut read_ok = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Some(children) = fs.dirs.get(&dir) else {
            continue; // read would error → not read_ok
        };
        read_ok.insert(dir.clone());
        for (name, ft) in children {
            let child = dir.join(name);
            edges.insert((dir.clone(), child.clone()));
            if *ft == RawFileType::Dir {
                stack.push(child);
            }
        }
    }
    (read_ok, edges)
}

// ── Recording visitor ────────────────────────────────────────────────

#[derive(Default)]
struct Recorded {
    /// id → path for every directory task the engine handled or the visitor created.
    id_to_path: HashMap<i64, PathBuf>,
    /// (parent_id, child_path) for every child seen.
    edges: Vec<(i64, PathBuf)>,
    /// Paths of directories whose read succeeded (visit_dir called).
    read_ok: BTreeSet<PathBuf>,
    /// Paths reported via visit_read_error, with whether it was a timeout.
    errors: Vec<(PathBuf, bool)>,
    /// Every id assigned to a child (to assert uniqueness).
    assigned_ids: Vec<i64>,
}

struct RecordingVisitor {
    next_id: AtomicI64,
    rec: StdMutex<Recorded>,
}

impl RecordingVisitor {
    fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1000),
            rec: StdMutex::new(Recorded::default()),
        }
    }
}

impl DirVisitor for RecordingVisitor {
    fn visit_dir(&self, dir: &DirTask, children: Vec<RawDirEntry>) -> Vec<DirTask> {
        let mut subdirs = Vec::new();
        let mut rec = self.rec.lock().unwrap_or_else(|e| e.into_inner());
        rec.id_to_path.insert(dir.id, dir.path.clone());
        rec.read_ok.insert(dir.path.clone());
        for child in children {
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            rec.assigned_ids.push(id);
            rec.edges.push((dir.id, child.path.clone()));
            rec.id_to_path.insert(id, child.path.clone());
            if child.file_type == RawFileType::Dir {
                subdirs.push(DirTask { path: child.path, id });
            }
        }
        subdirs
    }

    fn visit_read_error(&self, dir: &DirTask, err: &WalkReadError) {
        let mut rec = self.rec.lock().unwrap_or_else(|e| e.into_inner());
        rec.id_to_path.insert(dir.id, dir.path.clone());
        rec.errors
            .push((dir.path.clone(), matches!(err, WalkReadError::TimedOut)));
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn fast_cfg(num_threads: usize) -> WalkConfig {
    WalkConfig {
        num_threads,
        read_timeout: Duration::from_millis(50),
        watchdog_interval: Duration::from_millis(5),
        // The default budget (32) is far above any existing test's failure count,
        // so the give-up path stays out of the way here; its own test sets a small
        // budget deliberately.
        give_up_after: DEFAULT_GIVE_UP_AFTER,
    }
}

fn root_task(path: &str) -> DirTask {
    DirTask {
        path: PathBuf::from(path),
        id: 1,
    }
}

/// Translate recorded (parent_id, child_path) edges to (parent_path, child_path).
fn edges_by_path(rec: &Recorded) -> BTreeSet<(PathBuf, PathBuf)> {
    rec.edges
        .iter()
        .map(|(pid, child)| {
            (
                rec.id_to_path.get(pid).cloned().expect("parent id must be recorded"),
                child.clone(),
            )
        })
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────

#[test]
fn walks_full_tree_and_attributes_parents() {
    let mut b = TreeBuilder::default();
    b.dir(
        "/r",
        &[
            ("a", RawFileType::Dir),
            ("b", RawFileType::Dir),
            ("f.txt", RawFileType::File),
        ],
    )
    .dir("/r/a", &[("a1", RawFileType::Dir), ("g.txt", RawFileType::File)])
    .dir("/r/a/a1", &[("leaf.txt", RawFileType::File)])
    .dir("/r/b", &[("h.txt", RawFileType::File)]);
    let fs = b.build(HashSet::new(), Duration::ZERO);

    let visitor = Arc::new(RecordingVisitor::new());
    let stats = walk(
        root_task("/r"),
        fast_cfg(4),
        fs.clone().reader(),
        visitor.clone(),
        Arc::new(AtomicBool::new(false)),
    );

    let (ref_ok, ref_edges) = reference_walk(&fs, Path::new("/r"));
    let rec = visitor.rec.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(rec.read_ok, ref_ok, "every directory should be read exactly once");
    assert_eq!(
        edges_by_path(&rec),
        ref_edges,
        "parent→child edges must match the reference"
    );
    assert_eq!(stats.dirs_read, 4, "4 directories read (/r, /r/a, /r/a/a1, /r/b)");
    assert_eq!(stats.timed_out, 0);

    // Every assigned id is unique.
    let unique: HashSet<i64> = rec.assigned_ids.iter().copied().collect();
    assert_eq!(unique.len(), rec.assigned_ids.len(), "child ids must be unique");
}

#[test]
fn abandons_a_hung_dir_and_finishes_the_rest() {
    let mut b = TreeBuilder::default();
    b.dir("/r", &[("slow", RawFileType::Dir), ("ok", RawFileType::Dir)])
        .dir("/r/slow", &[("hidden.txt", RawFileType::File)]) // present but its read hangs
        .dir("/r/ok", &[("seen.txt", RawFileType::File)]);
    // `/r/slow`'s read blocks far longer than the 50 ms timeout.
    let hang: HashSet<PathBuf> = [PathBuf::from("/r/slow")].into_iter().collect();
    let fs = b.build(hang, Duration::from_secs(2));

    let visitor = Arc::new(RecordingVisitor::new());
    let start = Instant::now();
    let stats = walk(
        root_task("/r"),
        fast_cfg(4),
        fs.clone().reader(),
        visitor.clone(),
        Arc::new(AtomicBool::new(false)),
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(1),
        "walk must abandon the hung dir near the timeout, not wait ~2 s for it (elapsed {elapsed:?})",
    );
    assert_eq!(stats.timed_out, 1, "the hung dir is counted as timed out");

    let rec = visitor.rec.lock().unwrap_or_else(|e| e.into_inner());
    // The healthy sibling and root are fully indexed.
    assert!(rec.read_ok.contains(Path::new("/r")));
    assert!(rec.read_ok.contains(Path::new("/r/ok")));
    // The hung dir's subtree is NOT indexed, and it's reported as a timeout.
    assert!(
        !rec.read_ok.contains(Path::new("/r/slow")),
        "hung dir must not be marked read"
    );
    assert!(
        rec.errors.iter().any(|(p, timed)| p == Path::new("/r/slow") && *timed),
        "hung dir must be reported via visit_read_error as a timeout",
    );
}

#[test]
fn multiple_hung_dirs_do_not_starve_the_pool() {
    // More hung dirs than worker threads: with a fixed pool and no replacement,
    // this would deadlock; the watchdog's replacement workers must keep it moving,
    // and total time must be ~one timeout, not N sequential timeouts.
    let mut b = TreeBuilder::default();
    let hung_names = ["h0", "h1", "h2", "h3", "h4", "h5"];
    let mut root_children: Vec<(&str, RawFileType)> = vec![("ok", RawFileType::Dir)];
    for n in hung_names {
        root_children.push((n, RawFileType::Dir));
    }
    b.dir("/r", &root_children)
        .dir("/r/ok", &[("x.txt", RawFileType::File)]);
    let mut hang = HashSet::new();
    for n in hung_names {
        b.dir(&format!("/r/{n}"), &[("c.txt", RawFileType::File)]);
        hang.insert(PathBuf::from(format!("/r/{n}")));
    }
    let fs = b.build(hang, Duration::from_secs(2));

    let visitor = Arc::new(RecordingVisitor::new());
    let start = Instant::now();
    let stats = walk(
        root_task("/r"),
        fast_cfg(2), // fewer threads than hung dirs
        fs.clone().reader(),
        visitor.clone(),
        Arc::new(AtomicBool::new(false)),
    );
    let elapsed = start.elapsed();

    assert_eq!(stats.timed_out, hung_names.len() as u64, "all hung dirs time out");
    assert!(
        elapsed < Duration::from_secs(1),
        "replacement workers must keep the pool alive; total time ~one timeout, not N (elapsed {elapsed:?})",
    );
    let rec = visitor.rec.lock().unwrap_or_else(|e| e.into_inner());
    assert!(
        rec.read_ok.contains(Path::new("/r/ok")),
        "healthy dir still indexed under load"
    );
}

#[test]
fn io_error_dir_is_reported_and_pruned() {
    let mut b = TreeBuilder::default();
    // `/r/gone` is referenced as a dir but absent from the map → read errors.
    b.dir("/r", &[("gone", RawFileType::Dir), ("ok", RawFileType::Dir)])
        .dir("/r/ok", &[("y.txt", RawFileType::File)]);
    let fs = b.build(HashSet::new(), Duration::ZERO);

    let visitor = Arc::new(RecordingVisitor::new());
    let stats = walk(
        root_task("/r"),
        fast_cfg(4),
        fs.clone().reader(),
        visitor.clone(),
        Arc::new(AtomicBool::new(false)),
    );

    assert_eq!(stats.io_errors, 1, "the missing dir surfaces as an io error");
    assert_eq!(stats.timed_out, 0);
    let rec = visitor.rec.lock().unwrap_or_else(|e| e.into_inner());
    assert!(!rec.read_ok.contains(Path::new("/r/gone")));
    assert!(
        rec.errors.iter().any(|(p, timed)| p == Path::new("/r/gone") && !*timed),
        "missing dir reported as a non-timeout read error",
    );
    assert!(rec.read_ok.contains(Path::new("/r/ok")), "sibling still indexed");
}

#[test]
fn parallel_result_matches_serial_reference() {
    // A wider/deeper deterministic tree, walked with several threads, must produce
    // exactly the serial reference's dirs and edges — no dropped or misattributed
    // children under concurrency.
    let mut b = TreeBuilder::default();
    let mut top = Vec::new();
    for i in 0..8 {
        top.push((format!("d{i}"), RawFileType::Dir));
    }
    b.dirs
        .insert(PathBuf::from("/r"), top.iter().map(|(n, t)| (n.clone(), *t)).collect());
    for i in 0..8 {
        let d = format!("/r/d{i}");
        let mut kids = Vec::new();
        for j in 0..6 {
            kids.push((format!("s{j}"), RawFileType::Dir));
        }
        kids.push(("file.txt".to_string(), RawFileType::File));
        b.dirs.insert(PathBuf::from(&d), kids.clone());
        for j in 0..6 {
            b.dirs.insert(
                PathBuf::from(format!("{d}/s{j}")),
                vec![("leaf.txt".to_string(), RawFileType::File)],
            );
        }
    }
    let fs = b.build(HashSet::new(), Duration::ZERO);

    let visitor = Arc::new(RecordingVisitor::new());
    let stats = walk(
        root_task("/r"),
        fast_cfg(6),
        fs.clone().reader(),
        visitor.clone(),
        Arc::new(AtomicBool::new(false)),
    );

    let (ref_ok, ref_edges) = reference_walk(&fs, Path::new("/r"));
    let rec = visitor.rec.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(rec.read_ok, ref_ok, "parallel dirs must match serial reference");
    assert_eq!(
        edges_by_path(&rec),
        ref_edges,
        "parallel edges must match serial reference"
    );
    assert_eq!(stats.dirs_read, ref_ok.len() as u64);
    let unique: HashSet<i64> = rec.assigned_ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        rec.assigned_ids.len(),
        "no duplicate ids under concurrency"
    );
}

#[test]
fn cancellation_returns_promptly() {
    // With a hung tree, cancelling must return promptly via the watchdog, not wait
    // out the hang.
    let mut b = TreeBuilder::default();
    b.dir("/r", &[("slow", RawFileType::Dir)])
        .dir("/r/slow", &[("z.txt", RawFileType::File)]);
    let hang: HashSet<PathBuf> = [PathBuf::from("/r/slow")].into_iter().collect();
    let fs = b.build(hang, Duration::from_secs(5));

    let cancelled = Arc::new(AtomicBool::new(false));
    let visitor = Arc::new(RecordingVisitor::new());
    {
        let cancelled = cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            cancelled.store(true, Ordering::SeqCst);
        });
    }

    let start = Instant::now();
    let _stats = walk(
        root_task("/r"),
        WalkConfig {
            num_threads: 2,
            read_timeout: Duration::from_secs(10), // long, so only cancel can end it
            watchdog_interval: Duration::from_millis(5),
            give_up_after: DEFAULT_GIVE_UP_AFTER,
        },
        fs.clone().reader(),
        visitor,
        cancelled,
    );
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "cancel must end the walk promptly, not wait out the hang (elapsed {:?})",
        start.elapsed(),
    );
}

#[test]
fn gives_up_on_a_dead_subtree_and_keeps_walking_a_healthy_sibling() {
    // A dead mount: `/r/dead` lists OK but EVERY one of its many children fails to
    // read (like a disconnected File Provider returning ETIMEDOUT per descendant).
    // Without the give-up budget the walker probes all of them (the log-flood /
    // wasted-time bug); with it, the subtree is abandoned after ~N consecutive
    // failures and the rest are pruned unread. The healthy sibling `/r/healthy`
    // must still be walked in full, and the pruned dead dirs must be left
    // honest-stale — never marked read (so never false-completed or zeroed).
    const DEAD_CHILDREN: usize = 200;
    const HEALTHY_CHILDREN: usize = 20;
    const GIVE_UP_AFTER: usize = 4;
    const NUM_THREADS: usize = 2;

    let dead_names: Vec<String> = (0..DEAD_CHILDREN).map(|i| format!("d{i}")).collect();
    let healthy_names: Vec<String> = (0..HEALTHY_CHILDREN).map(|i| format!("h{i}")).collect();

    let mut b = TreeBuilder::default();
    b.dir("/r", &[("dead", RawFileType::Dir), ("healthy", RawFileType::Dir)]);
    // `/r/dead` lists OK, yielding many child dirs that are ABSENT from the map, so
    // each of their reads errors immediately (an IO-error dead subtree).
    let dead_children: Vec<(&str, RawFileType)> = dead_names.iter().map(|n| (n.as_str(), RawFileType::Dir)).collect();
    b.dir("/r/dead", &dead_children);
    // `/r/healthy` is fully present: it and all its children read OK.
    let healthy_children: Vec<(&str, RawFileType)> =
        healthy_names.iter().map(|n| (n.as_str(), RawFileType::Dir)).collect();
    b.dir("/r/healthy", &healthy_children);
    for n in &healthy_names {
        b.dir(&format!("/r/healthy/{n}"), &[("leaf.txt", RawFileType::File)]);
    }
    let fs = b.build(HashSet::new(), Duration::ZERO);

    let visitor = Arc::new(RecordingVisitor::new());
    let cfg = WalkConfig {
        num_threads: NUM_THREADS,
        read_timeout: Duration::from_millis(50),
        watchdog_interval: Duration::from_millis(5),
        give_up_after: GIVE_UP_AFTER,
    };
    let stats = walk(
        root_task("/r"),
        cfg,
        fs.clone().reader(),
        visitor.clone(),
        Arc::new(AtomicBool::new(false)),
    );

    let rec = visitor.rec.lock().unwrap_or_else(|e| e.into_inner());

    // The dead subtree was given up at least once (bounded work, one log line).
    assert!(
        stats.subtrees_abandoned >= 1,
        "the dead subtree must trip the give-up budget"
    );

    // Whole-subtree abandonment: only ~N dead children were ever probed; the vast
    // majority were pruned unread. "Consecutive" is loose under concurrency, so
    // allow a small per-thread slack, but it must be nowhere near DEAD_CHILDREN.
    let dead_probed = rec
        .errors
        .iter()
        .filter(|(p, _)| p.parent() == Some(Path::new("/r/dead")))
        .count();
    assert!(
        dead_probed <= GIVE_UP_AFTER + NUM_THREADS * 2,
        "dead children probed ({dead_probed}) must be bounded near the budget ({GIVE_UP_AFTER}), \
         not the whole {DEAD_CHILDREN}",
    );

    // Honest-stale: no dead child is marked read (they either erred or were pruned;
    // none is completed/known). The pruned majority are neither read nor even
    // error-reported — left silently unknown, exactly as a dir the scan never reached.
    assert!(
        !rec.read_ok.iter().any(|p| p.parent() == Some(Path::new("/r/dead"))),
        "no dead child may be marked read (honest-stale, never false-complete)",
    );

    // The healthy sibling is fully walked despite the dead subtree flooding the queue.
    assert!(rec.read_ok.contains(Path::new("/r/healthy")), "healthy root read");
    for n in &healthy_names {
        let p = PathBuf::from(format!("/r/healthy/{n}"));
        assert!(
            rec.read_ok.contains(&p),
            "healthy subtree must be fully indexed: {} missing",
            p.display(),
        );
    }
}
