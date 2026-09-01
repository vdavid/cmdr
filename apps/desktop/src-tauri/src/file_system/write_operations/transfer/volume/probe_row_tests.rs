//! Whether the in-flight table tells the truth about a SUBTREE that is fanning
//! out: one row per concurrently-writing leaf, and a declared width that is the
//! one the operation actually opens.
//!
//! The defect these pin came out of a user's diagnostic bundle. A folder of 282
//! files going to a NAS dumped `in_flight=1/1` while ten `write_from_stream`
//! calls were open at once, and its single row was the top-level DIRECTORY
//! carrying one leaf's byte count. A wedge investigation reads that table and
//! nothing else, so a table that names the folder instead of the ten files in
//! flight is the difference between finding the stuck file and guessing.
//!
//! Worse than cosmetic: `TaskProbe` is built on "one row, one write attempt"
//! (`arm_stall_abort` REPLACES the row's token, `set_bytes` STORES rather than
//! adds), so leaves sharing a row clobber each other's stall-abort signal and
//! keep resetting each other's stillness clock. A wedged leaf's token gets
//! replaced by a healthy sibling's, and the watchdog can never end its wait.
//!
//! ## How the table is photographed
//!
//! [`TablePhotographingDest`] calls `render_live_dump` from INSIDE
//! `write_from_stream`, after lingering until `rendezvous` writes are open at the
//! same moment. An in-memory write finishes in microseconds, so without the
//! linger two genuinely concurrent leaves are never open at the same instant and
//! every photo shows one row whatever the code does. ❌ Don't drop the linger to
//! make these faster — it's what makes the count mean anything. Same device, and
//! same reason, as `merge_window_tests.rs`.
//!
//! Shared fixtures live in `volume/move_test_support.rs` (`super::test_support`).

use std::sync::Mutex;

use super::super::super::transfer_probe::render_live_dump;
use super::super::faulty_volume::forward_volume_methods;
use super::test_support::make_state;
use super::*;
use crate::file_system::volume::{InMemoryVolume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;

/// How long a leaf lingers inside `write_from_stream` waiting for siblings to
/// join it, so the table can be photographed with several writes really open.
const LINGER: Duration = Duration::from_millis(250);

/// Bytes per leaf. Only has to be recognizable in a rendered row.
const LEAF_BYTES: usize = 4_096;

/// A destination that photographs the operation's in-flight table from inside a
/// write, once `rendezvous` writes are open at the same moment.
struct TablePhotographingDest {
    inner: Arc<InMemoryVolume>,
    operation_id: String,
    /// A real transport limit, so `transfer_concurrency` reads it as the binding
    /// cap for the pair (the `InMemoryVolume` source declares itself local).
    cap: usize,
    /// How many concurrent writes a leaf waits for before it stops lingering.
    rendezvous: usize,
    live: Arc<AtomicUsize>,
    dumps: Arc<Mutex<Vec<String>>>,
}

impl TablePhotographingDest {
    fn new(operation_id: &str, cap: usize, rendezvous: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
            operation_id: operation_id.to_owned(),
            cap,
            rendezvous,
            live: Arc::new(AtomicUsize::new(0)),
            dumps: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Every photo taken, in the order the writes took them.
    fn dumps(&self) -> Vec<String> {
        self.dumps.lock_ignore_poison().clone()
    }
}

impl Volume for TablePhotographingDest {
    forward_volume_methods!(
        inner => name, root, lane_key, list_directory, get_metadata, exists, is_directory, create_file,
        create_directory, create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        create_directory_errors_on_existing_dir, scan_for_copy, scan_for_copy_batch, open_read_stream,
        write_is_single_shot,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn operations_are_local(&self) -> bool {
        false
    }

    fn max_concurrent_ops(&self) -> usize {
        self.cap
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn crate::file_system::volume::VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.live.fetch_add(1, Ordering::Relaxed);
            let deadline = Instant::now() + LINGER;
            while self.live.load(Ordering::Relaxed) < self.rendezvous && Instant::now() < deadline {
                // allowed-test-sleep: the linger IS the subject — it's the fake write latency that makes overlap observable at all.
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            if let Some(dump) = render_live_dump(&self.operation_id, "photo") {
                self.dumps.lock_ignore_poison().push(dump);
            }
            let result = self.inner.write_from_stream(dest, size, stream, on_progress).await;
            self.live.fetch_sub(1, Ordering::Relaxed);
            result
        })
    }
}

/// Builds `/album` with `count` files on a fresh in-memory source.
async fn folder_of(count: usize) -> Arc<dyn Volume> {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/album")).await.expect("mkdir");
    for index in 0..count {
        let name = format!("/album/f-{index:02}.bin");
        source
            .create_file(Path::new(&name), &vec![index as u8; LEAF_BYTES])
            .await
            .expect("seed file");
    }
    source
}

/// The declared width from a dump's `in_flight=<open>/<width>` field.
fn declared_width(dump: &str) -> usize {
    dump.split_whitespace()
        .find_map(|token| token.strip_prefix("in_flight="))
        .and_then(|pair| pair.split('/').nth(1))
        .and_then(|width| width.parse().ok())
        .unwrap_or_else(|| panic!("every dump carries an in_flight field, got:\n{dump}"))
}

/// Every rendered task row, one per line after the header.
fn rows(dump: &str) -> Vec<&str> {
    dump.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .collect()
}

/// The rows for files INSIDE the folder, which is what a fanning-out subtree
/// should be showing.
fn leaf_rows(dump: &str) -> Vec<&str> {
    rows(dump).into_iter().filter(|row| row.contains("/album/f-")).collect()
}

/// The row for the top-level folder source itself, which every driver registers
/// alongside the leaves.
fn folder_row(dump: &str) -> &str {
    let mut matching = rows(dump).into_iter().filter(|row| !row.contains("/album/f-"));
    let row = matching
        .next()
        .unwrap_or_else(|| panic!("the top-level folder keeps a row of its own, got:\n{dump}"));
    assert!(
        matching.next().is_none(),
        "only the folder source itself may hold a non-leaf row, got:\n{dump}"
    );
    row
}

/// The photo taken with the most leaves open at once.
fn widest(dumps: &[String]) -> &String {
    dumps
        .iter()
        .max_by_key(|dump| leaf_rows(dump).len())
        .expect("a copy of a 12-file folder takes at least one photo")
}

/// Asserts the table a `width`-wide subtree walk must be showing: a row per
/// concurrently-writing leaf, each naming its own file, plus a folder row that
/// is holding no leaf's numbers.
fn assert_table_names_every_leaf(dumps: &[String], width: usize) {
    let dump = widest(dumps);
    assert_eq!(
        declared_width(dump),
        width,
        "the dump must declare the width the operation actually fans out to, got:\n{dump}"
    );

    let leaves = leaf_rows(dump);
    assert_eq!(
        leaves.len(),
        width,
        "every leaf writing at once owes its own row, so the dump must carry {width} of them, got:\n{dump}"
    );
    let mut named: Vec<&str> = leaves
        .iter()
        .map(|row| {
            row.split(" -> ")
                .next()
                .and_then(|left| left.rsplit(", ").next())
                .expect("a row renders as `... , <source> -> <dest>`")
        })
        .collect();
    named.sort_unstable();
    named.dedup();
    assert_eq!(
        named.len(),
        width,
        "two leaves sharing a row would clobber each other's stall-abort token, got:\n{dump}"
    );

    // `stream_pipe_file` calls `set_task_bytes(0, size)` before every write, so a
    // folder row carrying a leaf's size is proof the leaf reported into it.
    let folder = folder_row(dump);
    assert!(
        folder.contains("0/0 bytes"),
        "the folder's own row must carry no leaf's byte count, got:\n{dump}"
    );
    assert!(
        folder.contains("walking"),
        "the folder's own row names the walk, and stays in a phase the watchdog never acts on, got:\n{dump}"
    );

    // The walker holds no window slot (`strategy.rs::FileWindow`), so the header
    // must not measure it against one. Counted in, a perfectly healthy transfer
    // renders as `in_flight=5/4` and reads as a broken limiter — which is time
    // spent chasing the wrong thing in the middle of an incident.
    assert!(
        dump.contains(&format!("in_flight={width}/{width} walkers=1")),
        "the header must count only the window's writes and name the walker apart, got:\n{dump}"
    );
    assert!(
        folder.contains("(walker)"),
        "the walker's own row must say so, or the header's arithmetic can't be read back against the table, got:\n{dump}"
    );
}

/// A cross-volume MOVE of one folder is the same single-source shape a copy is,
/// and it opens the same window over the subtree. Its table has to say so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_moves_leaves_each_get_a_row_and_the_dump_declares_the_real_width() {
    let source = folder_of(12).await;
    let dest = TablePhotographingDest::new("probe-rows-move", 4, 4);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = move_volumes_with_progress(
        events,
        "probe-rows-move",
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &VolumeCopyConfig::default(),
    )
    .await;
    assert!(result.is_ok(), "the move must succeed, got {result:?}");

    assert_table_names_every_leaf(&dest.dumps(), 4);
}

/// The same table, from the SERIAL copy driver one folder also lands on. This is
/// the shape the user's bundle was taken from, and it stays pinned here so the
/// two drivers can't drift apart again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_copys_leaves_each_get_a_row_and_the_dump_declares_the_real_width() {
    let source = folder_of(12).await;
    let dest = TablePhotographingDest::new("probe-rows-copy", 4, 4);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = super::super::copy::copy_volumes_with_progress(
        events,
        "probe-rows-copy",
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &VolumeCopyConfig::default(),
    )
    .await;
    assert!(result.is_ok(), "the copy must succeed, got {result:?}");

    assert_table_names_every_leaf(&dest.dumps(), 4);
}
