//! What every interleaving test (`*_shuttle_tests.rs`) runs its body through:
//! Shuttle's PCT scheduler, a few hundred schedules, or one replayed schedule.
//!
//! A failure prints the schedule that broke it. Replay that one schedule with
//! `CMDR_SHUTTLE_REPLAY='<schedule>' cargo test -p cmdr-operations --features
//! shuttle --lib -- <test name>`, or rerun a whole exploration with
//! `SHUTTLE_RANDOM_SEED=<seed>`.

/// Schedules per test. Every recorded mutant (`*_shuttle_tests.mutants.json`)
/// goes red within the first handful, on every seed tried; 300 keeps each test
/// under a second.
const SCHEDULES: usize = 300;

/// PCT's bug depth: how many ordering constraints a bug may need at once. Three
/// covers "the writer falls behind while one thread acts and another reads".
const PCT_DEPTH: usize = 3;

/// Runs `body` under `SCHEDULES` schedules, or under the one schedule
/// `CMDR_SHUTTLE_REPLAY` names.
///
/// PCT rather than uniformly random: a uniform pick keeps the ledger's writer
/// thread close behind its producers, so "the flush returned before the writer
/// got there" survived 300 random schedules and fell to PCT's first few. PCT
/// starves a thread on purpose.
pub(crate) fn explore(body: impl Fn() + Send + Sync + 'static) {
    match std::env::var("CMDR_SHUTTLE_REPLAY") {
        Ok(schedule) => shuttle::replay(body, &schedule),
        Err(_) => shuttle::check_pct(body, SCHEDULES, PCT_DEPTH),
    }
}
