# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else (architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## One process per data dir

Two Cmdr processes on one data dir corrupt the index (two writers seeding the same entry-ID counter). `instance_lock.rs` makes that impossible: it takes an advisory `flock` on `<data dir>/.instance.lock` in the `setup` hook, before any database opens, and exits with a native alert if another process already holds it. Anything that relaunches the app against a live data dir (an updater path, a capture script, a test harness) must let the old process exit, or wait out the lock's ~5 s retry window. Mechanism, rationale, and the retry-window callers: [`/docs/tooling/instance-isolation.md`](../../../docs/tooling/instance-isolation.md) § Instance lock.
