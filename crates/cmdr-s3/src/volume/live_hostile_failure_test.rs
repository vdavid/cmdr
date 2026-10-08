//! Hostile cells for when things go wrong, against each real provider through
//! the `Volume` API: a Cancel at every point of an upload and a server-side
//! copy, a process killed mid-upload and swept by the next connect, and racing
//! writers. Each cell collects every miss per provider and fails once at the
//! end.
//!
//! Skips without `CMDR_S3_LIVE=1` (`live_support.rs`); the runner is
//! `apps/desktop/test/s3-servers/live.sh all live_hostile`. Findings:
//! `docs/notes/s3/live-hostile-2026-10.md`.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use cmdr_fs::volume::{ServerCopyProgress, StreamLength, Volume, VolumeError, WriteMode};

use super::live_hostile_support::*;
use super::live_support::*;
use super::testing::read_back;

/// ❗ A Cancel at every point of an upload (before the first part, mid-part,
/// right before the completion; one PUT mid-body and at its last piece), to a
/// free key and over an original: nothing published, the original byte for
/// byte, no unfinished upload on the server or in the ledger.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_cancel_uploads() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("cancel-uploads");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let floor = 5 * MIB as u64;
        volume.set_part_floor(floor);
        let original = b"the original, which must survive".to_vec();
        let multipart = 3 * floor + MIB as u64;
        let single = 2 * MIB as u64;
        let cases: [(&str, u64, u64); 5] = [
            ("multipart, before the first part", multipart, 0),
            ("multipart, mid second part", multipart, floor + 2 * MIB as u64),
            ("multipart, right before the completion", multipart, multipart),
            ("one PUT, mid-body", single, MIB as u64),
            ("one PUT, at its last piece", single, single),
        ];
        for (n, (what, size, at_bytes)) in cases.into_iter().enumerate() {
            for over in [false, true] {
                let key = format!("{prefix}{n}-{over}.bin");
                let path = at(&volume, &key);
                if over {
                    write(&volume, &path, WriteMode::CreateNew, original.clone())
                        .await
                        .expect("the original lands");
                    breathe().await;
                }
                let mode = if over {
                    WriteMode::CreateOrReplace
                } else {
                    WriteMode::CreateNew
                };
                let source = Box::new(PatternSource::new(size, n as u8));
                let outcome = write_cancelled_when(&volume, &path, mode, source, |written| written >= at_bytes).await;
                // A cut-off PUT's remains are removed after 150 and 300 ms.
                // allowed-test-sleep: the cut-off cleanup HEADs again after 150 and 300 ms; this outlasts it
                tokio::time::sleep(Duration::from_millis(500)).await;
                let left = live.length(&client, &key).await;
                let intact = if over {
                    read_back(&volume, &path).await == original
                } else {
                    left.is_none()
                };
                let uploads = leftover_uploads(&live, &prefix).await;
                let ledger = ledger_open(&volume, &prefix);
                // A Cancel after a PUT's last piece went out is too late: the
                // write then reports the file, which must be whole.
                let finished_whole = matches!(outcome, Ok(n) if n == size)
                    && n == 4
                    && pattern_mismatch(&volume, &path, size, n as u8).await.is_none();
                m.check(
                    (finished_whole || (matches!(outcome, Err(VolumeError::Cancelled(_))) && intact))
                        && uploads == Ok(Vec::new())
                        && ledger == 0,
                    &format!(
                        "cancel {what}, {}",
                        if over { "over an original" } else { "to a free key" }
                    ),
                    format!(
                        "{outcome:?}, {} {intact}, uploads {uploads:?}, ledger {ledger}",
                        if over { "original intact:" } else { "nothing published:" }
                    ),
                );
            }
        }
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// ❗ A Cancel at every point of a server-side copy: before the upload is
/// created, before the first part, between parts, after the last part
/// (before the completion), and on a one-request copy. The destination never
/// appears, the source is untouched, and no upload is left.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_cancel_server_copies() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("cancel-copies");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let floor = 5 * MIB as u64;
        volume.set_part_floor(floor);
        let size = 3 * floor + MIB as u64;
        let source = format!("{prefix}source.bin");
        write(
            &volume,
            &at(&volume, &source),
            WriteMode::CreateNew,
            pattern(size as usize, 3),
        )
        .await
        .expect("the source lands");
        write(
            &volume,
            &at(&volume, &format!("{prefix}small.txt")),
            WriteMode::CreateNew,
            b"small".to_vec(),
        )
        .await
        .expect("lands");
        let cases: [(&str, &str, CopyHook); 5] = [
            (
                "in parts, before the upload exists",
                "source.bin",
                CopyHook::new(Some(1), None),
            ),
            (
                "in parts, before the first part",
                "source.bin",
                CopyHook::new(Some(2), None),
            ),
            ("in parts, between parts", "source.bin", CopyHook::new(Some(3), None)),
            (
                "in parts, before the completion",
                "source.bin",
                CopyHook::new(None, Some(size)),
            ),
            ("one request", "small.txt", CopyHook::new(Some(1), None)),
        ];
        for (n, (what, from, hook)) in cases.into_iter().enumerate() {
            let to = format!("{prefix}copy-{n}");
            let outcome = volume
                .copy_on_server(
                    &volume,
                    &at(&volume, &format!("{prefix}{from}")),
                    &at(&volume, &to),
                    WriteMode::CreateNew,
                    &hook,
                )
                .await;
            let published = live.length(&client, &to).await;
            let source_kept = live.length(&client, &format!("{prefix}{from}")).await.is_some();
            let uploads = leftover_uploads(&live, &prefix).await;
            let ledger = ledger_open(&volume, &prefix);
            // GCS copies any size in one atomic `CopyObject`: only the
            // checkpoint before it can stop it, and a later Cancel is too late.
            let atomic_late = !client.profile().copies_in_parts && n > 0 && n < 4;
            if atomic_late {
                m.check(
                    outcome.as_ref().ok() == Some(&size) && published == Some(size),
                    &format!("cancel a server copy {what} (one atomic request here)"),
                    format!("{outcome:?}, published: {published:?}"),
                );
                live.delete_each(&client, &[to]).await;
                continue;
            }
            m.check(
                matches!(outcome, Err(VolumeError::Cancelled(_)))
                    && published.is_none()
                    && source_kept
                    && uploads == Ok(Vec::new())
                    && ledger == 0,
                &format!("cancel a server copy {what}"),
                format!(
                    "{outcome:?}, published: {published:?}, source kept: {source_kept}, uploads {uploads:?}, ledger {ledger}, checkpoints {}",
                    hook.checkpoints.load(Ordering::Relaxed)
                ),
            );
        }
        let intact = pattern_mismatch(&volume, &at(&volume, &source), size, 3).await;
        m.check(intact.is_none(), "the source after every cancel", format!("{intact:?}"));
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// A fresh directory for one cell's unfinished-upload record.
fn state_dir(label: &str) -> cmdr_fs::testing::TestDir {
    cmdr_fs::testing::TestDir::new(&format!("hostile_{label}"))
}

/// The upload ids the record under `dir` holds open, in the order started.
fn recorded_ids(dir: &Path) -> Vec<String> {
    let log = dir.join("s3").join("unfinished-uploads");
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let mut open: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split(' ');
        let op = fields.next();
        let Some(id) = fields.next_back() else { continue };
        let id = percent_encoding::percent_decode_str(id).decode_utf8_lossy().to_string();
        match op {
            Some("+") => open.push(id),
            Some("-") => open.retain(|open_id| *open_id != id),
            _ => {}
        }
    }
    open
}

/// Polls `until` every 200 ms for up to `limit`.
async fn wait_for(limit: Duration, mut until: impl FnMut() -> bool) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < limit {
        if until() {
            return true;
        }
        // allowed-test-sleep: the poll interval of this probe loop
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    until()
}

/// This test binary again, running only [`live_hostile_crash_child`] against
/// `live`: a separate process with its own in-flight registry.
fn spawn_child(live: &Live, dir: &Path, key: &str) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "volume::live_hostile_failure_test::live_hostile_crash_child",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CMDR_S3_LIVE_ONLY", live.name)
        .env("CMDR_S3_HOSTILE_CHILD_DIR", dir)
        .env("CMDR_S3_HOSTILE_CHILD_KEY", key)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the child starts")
}

/// The child half of the crash cells: a slow 40 MiB upload in 5 MiB parts,
/// recorded under the parent's state dir. A no-op unless the parent set its
/// variables.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_crash_child() {
    let (Ok(dir), Ok(key)) = (
        std::env::var("CMDR_S3_HOSTILE_CHILD_DIR"),
        std::env::var("CMDR_S3_HOSTILE_CHILD_KEY"),
    ) else {
        return;
    };
    let live = live_targets()
        .into_iter()
        .next()
        .expect("the parent names one provider");
    let volume = live
        .connect_with_state(Some(&live.bucket), Path::new(&dir))
        .await
        .expect("the child connects");
    volume.set_part_floor(5 * MIB as u64);
    let size = 40 * MIB as u64;
    let outcome = volume
        .write_from_stream(
            &at(&volume, &key),
            WriteMode::CreateNew,
            StreamLength::Known(size),
            Box::new(PatternSource::new(size, 11).paced(Duration::from_millis(300))),
            &|_| ControlFlow::Continue(()),
        )
        .await;
    report(&live, "CHILD RESULT", if outcome.is_ok() { "ok" } else { "failed" });
    report(&live, "CHILD DETAIL", format!("{outcome:?}"));
}

/// Whether upload `id` is gone: a one-byte part sent to it is refused. ❗ Not
/// a second abort: R2 and GCS answer an abort of an aborted upload with a
/// success, so that proves nothing (live, 2026-10-02).
async fn upload_gone(live: &Live, key: &str, id: &str) -> bool {
    live.upload_part(&live.client(), key, id, 1, vec![0]).await.is_err()
}

/// ❗ A process killed mid-upload leaves an unfinished upload; the next
/// connect's sweep aborts it and publishes nothing. An upload in flight in
/// this process is never swept by a second place's connect. Also reports what
/// a sweep does to an upload a SECOND live process is running on the same
/// record (Cmdr runs one process per data dir).
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_crash_recovery() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("crash");

        // 1. SIGKILL mid-upload, then a fresh connect sweeps.
        let dir = state_dir("killed");
        let key = format!("{prefix}killed.bin");
        let mut child = spawn_child(&live, &dir, &key);
        let recorded = wait_for(Duration::from_secs(90), || !recorded_ids(&dir).is_empty()).await;
        // Let a part or two land, so there's something to bill.
        // allowed-test-sleep: lets a part or two land before the SIGKILL; nothing observable marks that
        tokio::time::sleep(Duration::from_secs(4)).await;
        child.kill().expect("SIGKILL");
        let _ = child.wait();
        let ids = recorded_ids(&dir);
        let listed_before = live.uploads_under(&client, &prefix).await;
        m.check(
            recorded && ids.len() == 1,
            "a killed child left one recorded upload",
            format!(
                "{} recorded; server lists {:?}",
                ids.len(),
                listed_before.as_ref().map(Vec::len)
            ),
        );
        if let Some(id) = ids.first() {
            let volume = live
                .connect_with_state(Some(&live.bucket), &dir)
                .await
                .expect("connects");
            let swept_now = volume.sweep_unfinished_uploads().await;
            let gone = wait_for_async(Duration::from_secs(15), || upload_gone(&live, &key, id)).await;
            let listed_after = live.uploads_under(&client, &prefix).await;
            let published = live.length(&client, &key).await;
            let still_open = recorded_ids(&dir);
            let ledger = ledger_open(&volume, &prefix);
            m.check(
                gone && published.is_none() && still_open.is_empty() && ledger == 0 && listed_after == Ok(Vec::new()),
                "the next connect sweeps the killed upload",
                format!(
                    "explicit sweep aborted {swept_now} (the connect's own may have won), gone: {gone}, \
                     published: {published:?}, record open: {}, ledger {ledger}, server lists {listed_after:?}",
                    still_open.len()
                ),
            );
        }

        // 2. An upload in flight in this process survives another place's
        // connect and sweep.
        let dir = state_dir("in-flight");
        let key = format!("{prefix}in-flight.bin");
        let uploader = std::sync::Arc::new(
            live.connect_with_state(Some(&live.bucket), &dir)
                .await
                .expect("connects"),
        );
        uploader.set_part_floor(5 * MIB as u64);
        let size = 20 * MIB as u64;
        let task = {
            let uploader = uploader.clone();
            let path = at(&uploader, &key);
            tokio::spawn(async move {
                uploader
                    .write_from_stream(
                        &path,
                        WriteMode::CreateNew,
                        StreamLength::Known(size),
                        Box::new(PatternSource::new(size, 12).paced(Duration::from_millis(150))),
                        &|_| ControlFlow::Continue(()),
                    )
                    .await
            })
        };
        let started = wait_for(Duration::from_secs(60), || !recorded_ids(&dir).is_empty()).await;
        let second = live
            .connect_with_state(Some(&live.bucket), &dir)
            .await
            .expect("connects");
        let swept = second.sweep_unfinished_uploads().await;
        let finished = task.await.expect("the upload task");
        let intact = pattern_mismatch(&second, &at(&second, &key), size, 12).await;
        m.check(
            started && swept == 0 && finished.as_ref().ok() == Some(&size) && intact.is_none(),
            "an upload in flight in this process survives a second connect's sweep",
            format!("recorded: {started}, swept {swept}, upload {finished:?}, read back {intact:?}"),
        );

        // 3. A second LIVE process on the same record: reported, not judged.
        let dir = state_dir("two-processes");
        let key = format!("{prefix}other-process.bin");
        let child = spawn_child(&live, &dir, &key);
        let recorded = wait_for(Duration::from_secs(90), || !recorded_ids(&dir).is_empty()).await;
        let sweeper = live
            .connect_with_state(Some(&live.bucket), &dir)
            .await
            .expect("connects");
        let swept = sweeper.sweep_unfinished_uploads().await;
        let output = child.wait_with_output().expect("the child ends");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let child_ok = stdout.lines().any(|line| line.ends_with("CHILD RESULT: ok"));
        let detail = stdout
            .lines()
            .find(|line| line.contains("CHILD DETAIL"))
            .unwrap_or("no detail")
            .to_string();
        report(
            &live,
            "FINDING a sweep while another live process uploads on the same record",
            format!(
                "recorded: {recorded}, explicit sweep aborted {swept}, the other process's upload ok: {child_ok} ({detail})"
            ),
        );

        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// [`wait_for`] with an async condition.
async fn wait_for_async<F: Future<Output = bool>>(limit: Duration, mut until: impl FnMut() -> F) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < limit {
        if until().await {
            return true;
        }
        // allowed-test-sleep: the poll interval of this probe loop
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    until().await
}

/// A server-side copy's hook that replaces the source at checkpoint `at`
/// (before part `at - 1`), the way another writer would.
struct ReplaceSource<'a> {
    live: &'a Live,
    key: String,
    bytes: Vec<u8>,
    at: u32,
    checkpoints: AtomicU32,
}

impl ServerCopyProgress for ReplaceSource<'_> {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        let n = self.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        Box::pin(async move {
            if n == self.at {
                breathe().await;
                let answer = self.live.put(&self.live.client(), &self.key, &self.bytes, &[]).await;
                assert!(answer.status.is_success(), "replacing the source: {}", verdict(&answer));
            }
            ControlFlow::Continue(())
        })
    }
}

/// Whether `op` refuses atomically on this provider (a header) rather than
/// HEADing first.
fn atomic_no_overwrite(client: &crate::transport::S3Client, op: crate::profile::ConditionalOp) -> bool {
    client.profile().no_overwrite(op) != crate::profile::NoOverwrite::CheckThenWrite
}

/// ❗ Racing writers: two `CreateNew`s on one key (one PUT, and in parts), a
/// source replaced mid-copy (`SourceChanged`), deleting a folder's files while
/// another writer adds one, and renaming onto a taken name.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_races() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("races");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let floor = 5 * MIB as u64;
        volume.set_part_floor(floor);

        // Two `CreateNew`s on one free key, at once.
        for (shape, size, op) in [
            ("one PUT", 4096usize, crate::profile::ConditionalOp::Put),
            ("in parts", 11 * MIB, crate::profile::ConditionalOp::CompleteMultipart),
        ] {
            let rounds = if size > MIB { 1 } else { 3 };
            for round in 0..rounds {
                let key = format!("{prefix}clash-{size}-{round}.bin");
                let path = at(&volume, &key);
                let a = pattern(size, b'a');
                let b = pattern(size, b'b');
                let (got_a, got_b) = tokio::join!(
                    write(&volume, &path, WriteMode::CreateNew, a.clone()),
                    write(&volume, &path, WriteMode::CreateNew, b.clone())
                );
                let stored = read_back(&volume, &path).await;
                let winner_ok = (stored == a && got_a.is_ok()) || (stored == b && got_b.is_ok());
                let both = got_a.is_ok() && got_b.is_ok();
                let losers_typed = [&got_a, &got_b]
                    .iter()
                    .all(|got| matches!(got, Ok(_) | Err(VolumeError::AlreadyExists(_))));
                let atomic = atomic_no_overwrite(&client, op);
                m.check(
                    winner_ok && losers_typed && !(both && atomic),
                    &format!("two CreateNew {shape} on one key, round {round}"),
                    format!(
                        "{:?} / {:?}, stored {}, {}",
                        got_a.as_ref().map(|_| "ok"),
                        got_b.as_ref().map(|_| "ok"),
                        if stored == a {
                            "A"
                        } else if stored == b {
                            "B"
                        } else {
                            "NEITHER"
                        },
                        if both {
                            "BOTH reported ok (check-then-write's blind window)"
                        } else {
                            "one refused"
                        }
                    ),
                );
                let uploads = leftover_uploads(&live, &key).await;
                m.check(
                    uploads == Ok(Vec::new()),
                    &format!("no upload left by the {shape} clash"),
                    format!("{uploads:?}"),
                );
            }
        }

        // A source replaced between copy parts.
        let size = 3 * floor + MIB as u64;
        let source = format!("{prefix}moving-source.bin");
        write(
            &volume,
            &at(&volume, &source),
            WriteMode::CreateNew,
            pattern(size as usize, 1),
        )
        .await
        .expect("the source lands");
        let replacement = pattern(MIB, 2);
        let hook = ReplaceSource {
            live: &live,
            key: source.clone(),
            bytes: replacement.clone(),
            at: 3,
            checkpoints: AtomicU32::new(0),
        };
        let to = format!("{prefix}moving-copy.bin");
        let copied = volume
            .copy_on_server(
                &volume,
                &at(&volume, &source),
                &at(&volume, &to),
                WriteMode::CreateNew,
                &hook,
            )
            .await;
        let published = live.length(&client, &to).await;
        let source_now = live.length(&client, &source).await;
        let uploads = leftover_uploads(&live, &prefix).await;
        if client.profile().copies_in_parts {
            m.check(
                matches!(copied, Err(VolumeError::SourceChanged(_)))
                    && published.is_none()
                    && source_now == Some(MIB as u64)
                    && uploads == Ok(Vec::new()),
                "a source replaced mid-copy",
                format!("{copied:?}, published {published:?}, source now {source_now:?}, uploads {uploads:?}"),
            );
        } else {
            report(
                &live,
                "a source replaced mid-copy",
                format!("n/a: one atomic CopyObject here ({copied:?})"),
            );
        }

        // Delete a folder's listed files while another writer adds one.
        for n in 0..20 {
            live.put(&client, &format!("{prefix}busy/f{n:02}.txt"), b"x", &[]).await;
        }
        let busy = at(&volume, &format!("{prefix}busy"));
        let listed: Vec<PathBuf> = volume
            .list_directory(&busy, None)
            .await
            .expect("lists")
            .into_iter()
            .map(|e| PathBuf::from(e.path))
            .collect();
        let late = at(&volume, &format!("{prefix}busy/late.txt"));
        let (deleted, added) = tokio::join!(
            volume.delete_files(&listed),
            write(&volume, &late, WriteMode::CreateNew, b"late".to_vec())
        );
        let left = volume
            .list_directory(&busy, None)
            .await
            .map(|rows| rows.into_iter().map(|e| e.name).collect::<Vec<_>>());
        m.check(
            listed.len() == 20
                && deleted.iter().all(Result::is_ok)
                && added.is_ok()
                && left.as_ref().ok() == Some(&vec!["late.txt".to_string()]),
            "delete a folder's files while another writer adds one",
            format!(
                "{} listed, deletes ok: {}, add {added:?}, left {left:?}",
                listed.len(),
                deleted.iter().all(Result::is_ok)
            ),
        );

        // Delete an emptied folder while a writer puts a file in it.
        let lonely = at(&volume, &format!("{prefix}lonely"));
        volume.create_directory(&lonely).await.expect("a folder");
        let inside = at(&volume, &format!("{prefix}lonely/new.txt"));
        let (removed, wrote) = tokio::join!(
            volume.delete(&lonely),
            write(&volume, &inside, WriteMode::CreateNew, b"new".to_vec())
        );
        let survived = read_back(&volume, &inside).await == b"new";
        m.check(
            wrote.is_ok() && survived,
            "a file written while its folder is deleted survives",
            format!("delete {removed:?}, write {wrote:?}, file intact: {survived}"),
        );

        // Rename onto a taken name, then forced.
        let a = at(&volume, &format!("{prefix}a.txt"));
        let b = at(&volume, &format!("{prefix}b.txt"));
        write(&volume, &a, WriteMode::CreateNew, b"from a".to_vec())
            .await
            .expect("a");
        write(&volume, &b, WriteMode::CreateNew, b"from b".to_vec())
            .await
            .expect("b");
        let refused = volume.rename(&a, &b, false).await;
        let both_kept = read_back(&volume, &a).await == b"from a" && read_back(&volume, &b).await == b"from b";
        m.check(
            matches!(refused, Err(VolumeError::AlreadyExists(_))) && both_kept,
            "rename onto a taken name",
            format!("{refused:?}, both kept: {both_kept}"),
        );
        breathe().await;
        let forced = volume.rename(&a, &b, true).await;
        let b_now = read_back(&volume, &b).await;
        let a_gone = matches!(volume.get_metadata(&a).await, Err(VolumeError::NotFound(_)));
        m.check(
            forced.is_ok() && b_now == b"from a" && a_gone,
            "rename onto a taken name, forced",
            format!(
                "{forced:?}, b reads {:?}, a gone: {a_gone}",
                String::from_utf8_lossy(&b_now)
            ),
        );

        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}
