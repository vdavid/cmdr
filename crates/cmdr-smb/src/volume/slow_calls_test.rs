//! Which slow metadata calls reach the log, and what the line says.

use super::*;

const SHARE: &str = "naspi";

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// An ordinary round trip, however many of them, says nothing.
#[test]
fn a_call_under_a_second_says_nothing() {
    let log = SlowCallLog::new();
    assert_eq!(log.line_at(SHARE, "get_metadata", ms(999), Instant::now()), None);
}

/// The first stall is logged the moment it ends, naming the call, the share,
/// and how long the server held it.
#[test]
fn the_first_stall_on_a_share_is_logged_at_once() {
    let log = SlowCallLog::new();
    let line = log
        .line_at(SHARE, "get_metadata", ms(3_210), Instant::now())
        .expect("a 3.2 s stat is worth a line");
    assert!(line.contains("get_metadata"), "{line}");
    assert!(line.contains("share=naspi"), "{line}");
    assert!(line.contains("3210 ms"), "{line}");
}

/// A bad NAS can stall hundreds of calls a minute: they fold into one line per
/// minute that counts them and names the worst, so the log can't flood.
#[test]
fn a_run_of_stalls_rolls_up_and_names_the_slowest() {
    let log = SlowCallLog::new();
    let start = Instant::now();
    assert!(log.line_at(SHARE, "get_metadata", ms(1_500), start).is_some());
    assert_eq!(
        log.line_at(SHARE, "list_directory", ms(6_000), start + ms(10_000)),
        None
    );
    assert_eq!(log.line_at(SHARE, "exists", ms(1_200), start + ms(20_000)), None);

    let line = log
        .line_at(SHARE, "is_directory", ms(1_100), start + ms(61_000))
        .expect("a minute on, the rollup speaks");
    assert!(line.contains("×3"), "three stalls since the last line: {line}");
    assert!(line.contains("slowest 6000 ms"), "{line}");
}

/// Each share keeps its own window, so a busy one can't swallow another's first stall.
#[test]
fn a_busy_share_does_not_swallow_another_shares_first_stall() {
    let log = SlowCallLog::new();
    let now = Instant::now();
    assert!(log.line_at("busy", "get_metadata", ms(2_000), now).is_some());
    assert!(log.line_at("quiet", "get_metadata", ms(2_000), now).is_some());
}
