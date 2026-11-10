//! The late-tick evidence line: what it says, from synthetic snapshots, and
//! that the real reading produces one.

use std::time::Instant;

use cmdr_fs::process_cpu::ThreadSample;

use super::*;

fn thread(id: u64, name: &str, user_ms: u64) -> ThreadSample {
    ThreadSample {
        id,
        name: Some(name.to_owned()),
        user: Duration::from_millis(user_ms),
        system: Duration::ZERO,
    }
}

fn snapshot(taken_at: Instant, threads: Vec<ThreadSample>) -> CpuSnapshot {
    let total: Duration = threads.iter().map(|t| t.user + t.system).sum();
    CpuSnapshot {
        taken_at,
        process_user: total,
        process_system: Duration::ZERO,
        threads,
        wakeups: None,
        counters: None,
    }
}

/// The shape ERR-P4GWB needed: across a ten-minute gap the workers spent
/// nothing while the index writer kept working, which reads as "workers parked
/// in blocking calls" at a glance.
#[test]
fn the_line_names_each_thread_group_s_share_across_the_gap() {
    let start_at = Instant::now();
    let start = snapshot(
        start_at,
        vec![
            thread(1, "tokio-runtime-worker", 1_000),
            thread(2, "tokio-runtime-worker", 1_000),
            thread(3, "index-writer", 1_000),
        ],
    );
    let end = snapshot(
        start_at + Duration::from_secs(600),
        vec![
            thread(1, "tokio-runtime-worker", 1_000),
            thread(2, "tokio-runtime-worker", 1_000),
            thread(3, "index-writer", 7_000),
        ],
    );

    let line = render(&CpuCensus::between(&start, &end, 0));

    assert!(line.contains("over the last 600s"), "{line}");
    assert!(line.contains("process used 1.0% of one core"), "{line}");
    assert!(line.contains("threads 3→3"), "{line}");
    assert!(line.contains("index-writer ×1 1.0%"), "{line}");
    assert!(line.contains("tokio-runtime-worker ×2 0.0%"), "{line}");
}

/// Off macOS there's no reading, and the witness says nothing rather than
/// something misleading; on macOS the real kernel reading makes a line.
#[test]
fn the_witness_describes_the_real_process_where_it_can_read_it() {
    let mut witness = CpuWitness::default();
    witness.refresh_if_due(Duration::ZERO);
    let line = witness.describe_since_baseline(Duration::from_secs(1));
    if cfg!(target_os = "macos") {
        assert!(line.contains("of one core"), "{line}");
    } else {
        assert!(line.is_empty(), "{line}");
    }
}
