//! Carrying an upload on from its last checkpoint, against a real server
//! (requires Docker SMB containers): `checkpointed_write.rs`.
//!
//! Each cell writes a staging temp with a small checkpoint interval, fails it
//! on the transport part-way (a source that times out, so the share stays up
//! for the assertions), maybe tampers with the temp the way something else on
//! the server could, and then carries it on. The data-safety bar: a carried-on
//! temp holds exactly the source's bytes, or the continuation refuses with
//! `NotSupported` and the engine runs the file again. Never a third outcome.
//!
//! The engine-level cell (a real outage, through the transfer pipeline) is the
//! app's `operations_host/backend_suites/smb_outage_test.rs`; the checkpoint's
//! throughput cost is `smb_integration_checkpoint_cost` below, `--nocapture`.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, StreamWritten, WriteMode};
use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::Mutex;

const MIB: usize = 1024 * 1024;
/// The checkpoint interval every cell writes under.
const EVERY: usize = 2 * MIB;
/// Past several checkpoints, and past Samba's 8 MiB `max_write`: once a
/// write has measured the loopback uplink, a staging temp that fits one
/// WRITE goes as a single compound frame (`one_frame_write_limit`) and never
/// streams, so a smaller file wouldn't exercise this path at all.
const SIZE: usize = 20 * MIB + 12_345;
/// Where the failing source stops: past a few checkpoints, between two.
const FAIL_AT: usize = 9 * MIB + 300_000;
const CHUNK: usize = 256 * 1024;

/// Bytes that differ at every offset worth telling apart.
fn payload(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i.wrapping_mul(31) ^ (i >> 9)) as u8).collect()
}

/// `data` in `CHUNK`s, then a timeout (a transport fault) once `fail_at`
/// bytes are out, the way a source whose share dropped ends.
struct FailingSource {
    data: Vec<u8>,
    pos: usize,
    fail_at: Option<usize>,
}

impl FailingSource {
    fn boxed(data: &[u8], fail_at: Option<usize>) -> Box<dyn VolumeReadStream> {
        Box::new(Self {
            data: data.to_vec(),
            pos: 0,
            fail_at,
        })
    }
}

impl VolumeReadStream for FailingSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.fail_at.is_some_and(|at| self.pos >= at) {
                return Some(Err(VolumeError::ConnectionTimeout(
                    "the source stopped answering".into(),
                )));
            }
            if self.pos >= self.data.len() {
                return None;
            }
            let end = (self.pos + CHUNK)
                .min(self.data.len())
                .min(self.fail_at.unwrap_or(usize::MAX).max(self.pos + 1));
            let chunk = self.data[self.pos..end].to_vec();
            self.pos = end;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.data.len() as u64)
    }

    fn bytes_read(&self) -> u64 {
        self.pos as u64
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

/// A scratch dir and a temp's path inside it.
async fn setup() -> (SmbVolume, String, PathBuf, Vec<u8>) {
    let vol = make_docker_volume().await;
    set_checkpoint_every(&vol, EVERY as u64);
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let temp = PathBuf::from(format!("{dir}/.cmdr-tmp-carry-on"));
    (vol, dir, temp, payload(SIZE))
}

/// Writes `data` into `temp` until the source fails, and answers what the
/// write said it could carry on from: the last `continues_from` it reported.
async fn fail_part_way(vol: &SmbVolume, temp: &Path, data: &[u8]) -> u64 {
    let heard = Mutex::new(Vec::<StreamWriteProgress>::new());
    let on_progress = |progress: StreamWriteProgress| {
        heard.lock().unwrap().push(progress);
        ControlFlow::Continue(())
    };
    let result = vol
        .write_from_stream(
            temp,
            WriteMode::CreateNew,
            StreamLength::Known(data.len() as u64),
            FailingSource::boxed(data, Some(FAIL_AT)),
            &on_progress,
        )
        .await;
    assert!(
        matches!(result, Err(VolumeError::ConnectionTimeout(_))),
        "the source's timeout fails the write: {result:?}"
    );
    let heard = heard.into_inner().unwrap();
    let mut proven = 0;
    for progress in &heard {
        assert!(
            progress.continues_from <= progress.bytes_written,
            "never past the live count"
        );
        assert!(progress.continues_from >= proven, "a proven prefix never shrinks");
        proven = progress.continues_from;
    }
    assert!(
        proven >= 2 * EVERY as u64,
        "checkpoints every MiB reach past two: {proven}"
    );
    assert!(
        proven as usize <= FAIL_AT,
        "nothing past what the source yielded is proven"
    );
    assert!(
        vol.exists(temp).await,
        "a transport fault past a checkpoint keeps the temp"
    );
    let smb_path = vol.to_smb_path(temp).unwrap();
    assert!(vol.inner.write_checkpoints.holds(&smb_path), "with its checkpoint kept");
    proven
}

async fn carry_on(vol: &SmbVolume, temp: &Path, data: &[u8], at: u64) -> Result<StreamWritten, VolumeError> {
    let no_progress = &|_: StreamWriteProgress| ControlFlow::Continue(());
    vol.continue_write_from_stream(
        temp,
        at,
        StreamLength::Known(data.len() as u64),
        FailingSource::boxed(&data[at as usize..], None),
        no_progress,
    )
    .await
}

async fn read_all(vol: &SmbVolume, path: &Path) -> Vec<u8> {
    let mut stream = vol.open_read_stream(path).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    bytes
}

/// The share's own session, to change the temp behind the volume's back.
async fn raw(vol: &SmbVolume) -> (Arc<Tree>, smb2::client::Connection) {
    let lease = vol.clone_session().await.unwrap();
    (lease.tree, lease.conn)
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_upload_cut_on_the_transport_carries_on_from_its_checkpoint() {
    let (vol, dir, temp, data) = setup().await;
    let at = fail_part_way(&vol, &temp, &data).await;

    let written = carry_on(&vol, &temp, &data, at).await.expect("the temp carries on");
    assert_eq!(written.bytes, data.len() as u64, "counted from the file's first byte");
    assert!(
        read_all(&vol, &temp).await == data,
        "the temp holds the source, byte for byte"
    );
    let smb_path = vol.to_smb_path(&temp).unwrap();
    assert!(
        !vol.inner.write_checkpoints.holds(&smb_path),
        "a landed write keeps nothing"
    );

    // Nor does one that never failed, past many checkpoints.
    let whole = PathBuf::from(format!("{dir}/.cmdr-tmp-whole"));
    let no_progress = &|_: StreamWriteProgress| ControlFlow::Continue(());
    vol.write_from_stream(
        &whole,
        WriteMode::CreateNew,
        StreamLength::Known(SIZE as u64),
        FailingSource::boxed(&data, None),
        no_progress,
    )
    .await
    .unwrap();
    assert!(read_all(&vol, &whole).await == data);
    let smb_path = vol.to_smb_path(&whole).unwrap();
    assert!(
        !vol.inner.write_checkpoints.holds(&smb_path),
        "a whole write keeps nothing"
    );
    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_carry_on_overwrites_and_trims_what_landed_past_the_checkpoint() {
    let (vol, dir, temp, data) = setup().await;
    let at = fail_part_way(&vol, &temp, &data).await;
    // Past the checkpoint: junk where the rest goes, and a tail past the
    // file's last byte (WRITEs that landed after the checkpoint and before the
    // drop would sit there, and a longer file is what a resume expects).
    let smb_path = vol.to_smb_path(&temp).unwrap();
    let (tree, conn) = raw(&vol).await;
    let mut scribble = tree.create_file_writer_at(conn, &smb_path, at).await.unwrap();
    scribble
        .write_chunk(&vec![0xEE; SIZE - at as usize + 3 * MIB])
        .await
        .unwrap();
    scribble.finish().await.unwrap();

    carry_on(&vol, &temp, &data, at)
        .await
        .expect("a longer temp carries on");
    assert!(
        read_all(&vol, &temp).await == data,
        "every byte past the checkpoint is the source's, and nothing is past its end"
    );
    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_carry_on_from_an_earlier_byte_than_the_checkpoint_holds() {
    let (vol, dir, temp, data) = setup().await;
    let at = fail_part_way(&vol, &temp, &data).await;
    // The engine heard an earlier checkpoint than the one taken on the way out.
    carry_on(&vol, &temp, &data, at - EVERY as u64)
        .await
        .expect("any flushed prefix carries on");
    assert!(read_all(&vol, &temp).await == data);
    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_carry_on_refuses_a_temp_it_cant_prove() {
    let (vol, dir, _, data) = setup().await;
    let temp = |label: &str| PathBuf::from(format!("{dir}/.cmdr-tmp-{label}"));

    // Past the checkpoint: nothing proves those bytes.
    let past_temp = temp("past");
    let at = fail_part_way(&vol, &past_temp, &data).await;
    let past = carry_on(&vol, &past_temp, &data, at + 1).await;
    assert!(
        matches!(past, Err(VolumeError::NotSupported)),
        "past the checkpoint: {past:?}"
    );

    // Truncated in place: the same file, missing checkpointed bytes.
    let truncated_temp = temp("truncated");
    let at = fail_part_way(&vol, &truncated_temp, &data).await;
    let smb_path = vol.to_smb_path(&truncated_temp).unwrap();
    let (tree, conn) = raw(&vol).await;
    let mut truncating = tree.create_file_writer(conn, &smb_path).await.unwrap();
    truncating.write_chunk(&data[..MIB]).await.unwrap();
    truncating.finish().await.unwrap();
    let truncated = carry_on(&vol, &truncated_temp, &data, at).await;
    assert!(
        matches!(truncated, Err(VolumeError::NotSupported)),
        "truncated: {truncated:?}"
    );

    // Replaced: deleted and written again at its name, as long as before.
    // The fixture hands the new file a new inode; "reborn" below is one
    // that got the old one back.
    let replaced_temp = temp("replaced");
    let at = fail_part_way(&vol, &replaced_temp, &data).await;
    let smb_path = vol.to_smb_path(&replaced_temp).unwrap();
    let (tree, mut conn) = raw(&vol).await;
    tree.delete_file(&mut conn, &smb_path).await.unwrap();
    let mut replacing = tree.create_file_writer(conn, &smb_path).await.unwrap();
    replacing.write_chunk(&vec![0x5A; SIZE]).await.unwrap();
    replacing.finish().await.unwrap();
    let replaced = carry_on(&vol, &replaced_temp, &data, at).await;
    assert!(
        matches!(replaced, Err(VolumeError::NotSupported)),
        "replaced: {replaced:?}"
    );

    // Reborn: the same identity with another creation time, which is how a
    // replacement that got the old inode back looks.
    let reborn_temp = temp("reborn");
    let at = fail_part_way(&vol, &reborn_temp, &data).await;
    let smb_path = vol.to_smb_path(&reborn_temp).unwrap();
    let (tree, mut conn) = raw(&vol).await;
    let elsewhen = std::time::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    tree.set_times(&mut conn, &smb_path, smb2::FileTimes::new().set_created(elsewhen))
        .await
        .unwrap();
    let reborn = carry_on(&vol, &reborn_temp, &data, at).await;
    assert!(matches!(reborn, Err(VolumeError::NotSupported)), "reborn: {reborn:?}");

    // Gone.
    let gone_temp = temp("gone");
    let at = fail_part_way(&vol, &gone_temp, &data).await;
    let smb_path = vol.to_smb_path(&gone_temp).unwrap();
    let (tree, mut conn) = raw(&vol).await;
    tree.delete_file(&mut conn, &smb_path).await.unwrap();
    let gone = carry_on(&vol, &gone_temp, &data, at).await;
    assert!(matches!(gone, Err(VolumeError::NotSupported)), "gone: {gone:?}");
    assert!(!vol.exists(&gone_temp).await, "a refusal creates nothing");

    // Nothing kept: a temp this volume never checkpointed.
    let never = carry_on(&vol, &temp("never"), &data, MIB as u64).await;
    assert!(matches!(never, Err(VolumeError::NotSupported)), "never kept: {never:?}");
    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_failure_that_isnt_the_transport_keeps_nothing() {
    let (vol, dir, temp, data) = setup().await;
    // A source refusal, past several checkpoints: the temp goes as before.
    struct Refusing(FailingSource);
    impl VolumeReadStream for Refusing {
        fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
            Box::pin(async move {
                match self.0.next_chunk().await {
                    Some(Err(_)) => Some(Err(VolumeError::SourceChanged("refused".into()))),
                    other => other,
                }
            })
        }
        fn total_size(&self) -> StreamLength {
            self.0.total_size()
        }
        fn bytes_read(&self) -> u64 {
            self.0.bytes_read()
        }
        fn modified_at(&self) -> Option<std::time::SystemTime> {
            None
        }
    }
    let source = Box::new(Refusing(FailingSource {
        data: data.clone(),
        pos: 0,
        fail_at: Some(FAIL_AT),
    }));
    let no_progress = &|_: StreamWriteProgress| ControlFlow::Continue(());
    let result = vol
        .write_from_stream(
            &temp,
            WriteMode::CreateNew,
            StreamLength::Known(SIZE as u64),
            source,
            no_progress,
        )
        .await;
    assert!(matches!(result, Err(VolumeError::SourceChanged(_))), "{result:?}");
    assert!(!vol.exists(&temp).await, "the partial goes");
    let smb_path = vol.to_smb_path(&temp).unwrap();
    assert!(!vol.inner.write_checkpoints.holds(&smb_path), "and keeps no checkpoint");
    ensure_clean(&vol, &dir).await;
}

/// Not a check: what a checkpoint costs, for `DETAILS.md` § "Carrying an
/// upload on". Run with `--nocapture`; per interval it prints the median
/// throughput and what one checkpoint added over the same temp written with
/// none. The intervals interleave within each round, so drift in the Docker
/// VM lands on all of them alike. `CHECKPOINT_COST_MIB` (256) and
/// `CHECKPOINT_COST_ROUNDS` (5) size it.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
#[allow(
    clippy::print_stdout,
    reason = "a measurement, printed by design (run with --nocapture)"
)]
async fn smb_integration_checkpoint_cost() {
    let env = |name: &str, default: usize| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let size = env("CHECKPOINT_COST_MIB", 256) * MIB;
    let rounds = env("CHECKPOINT_COST_ROUNDS", 5);
    let data = payload(size);
    // 0: no checkpoint at all, on the same kind of temp.
    let intervals_mib = [0u64, 1, 4, 16, 64];
    let mut secs: Vec<Vec<f64>> = vec![Vec::new(); intervals_mib.len()];
    for round in 0..rounds {
        for (i, &every_mib) in intervals_mib.iter().enumerate() {
            let every = match every_mib {
                0 => u64::MAX / 2,
                n => n * MIB as u64,
            };
            set_checkpoint_every(&vol, every);
            let name = format!("{dir}/.cmdr-tmp-cost-{every_mib}-{round}");
            let started = std::time::Instant::now();
            let no_progress = &|_: StreamWriteProgress| ControlFlow::Continue(());
            vol.write_from_stream(
                Path::new(&name),
                WriteMode::CreateNew,
                StreamLength::Known(size as u64),
                FailingSource::boxed(&data, None),
                no_progress,
            )
            .await
            .unwrap();
            secs[i].push(started.elapsed().as_secs_f64());
            let _ = vol.delete(Path::new(&name)).await;
        }
    }
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let medians: Vec<f64> = secs.into_iter().map(median).collect();
    for (i, &every_mib) in intervals_mib.iter().enumerate() {
        let per_checkpoint = match every_mib {
            0 => String::from("(baseline)"),
            n => {
                let count = (size as u64 / (n * MIB as u64)) as f64;
                format!("{:+.1} ms per checkpoint", (medians[i] - medians[0]) * 1e3 / count)
            }
        };
        println!(
            "checkpoint every {every_mib:>2} MiB: {:>7.1} MB/s, {per_checkpoint} (median of {rounds}, {} MiB)",
            size as f64 / medians[i] / 1e6,
            size / MIB
        );
    }
    ensure_clean(&vol, &dir).await;
}
