# Usage: details

## What the file holds

`usage.json`, in the app data dir beside `favorites.json` and `install-ids.json`:

```json
{
  "_schemaVersion": 1,
  "launchDays": ["2026-09-07", "2026-09-08", "2026-09-09"]
}
```

Days are `YYYY-MM-DD` strings in the person's LOCAL calendar, appended in the order they were first seen. Nothing else
lives in the file: no timestamps, no counts, no session lengths. That's deliberate, and it's what the privacy-policy
line promises.

### Decision: `_schemaVersion`, with the leading underscore

The house convention, shared by `favorites/store.rs` and `recents/persistence.rs` (which asserts on the literal key).
The original brief said `schemaVersion`; consistency with the four files already in the data dir won. The file has no
readers outside Cmdr, so there was nothing to break.

## Data flow

1. `lib.rs` setup tail calls `usage::record_launch(&data_dir)` inside the block that already resolved
   `config::resolved_app_data_dir`.
2. `ledger::record` reads the file, asks the pure `appended(days, today)` whether today is new, and writes through
   `config::durable_write_json` only when it is.
3. Later, the frontend asks `get_launch_day_count` (`../commands/usage.rs`), which reads the file on the blocking pool
   under the standard 2 s read deadline and counts distinct days. Anything that goes wrong answers 0.

### Decision: synchronous, on the setup thread

`record_launch` reads and writes inline rather than on `spawn_blocking`.

- The read is a few hundred bytes. The write happens at most once per calendar day, so on most launches there is no
  write at all.
- `install_id::init` sets the precedent right beside it: a durable JSON write on the same setup thread.
- Doing it inline is what makes `launch_day_count` unable to race the append. With a spawned write, a hint that asked
  early would see yesterday's count, and the whole point of the ledger is that a threshold like "three days" is exact.

If a startup profile ever shows this, moving the write off-thread needs a cached in-memory snapshot to keep the read
seam race-free; don't move it without one.

### Decision: not a `RecentsFile`

`recents/` is a capped, deduped, newest-first list with a per-call cap and an id per entry. The ledger is none of those:
it's uncapped, append-only, ordered by first sight, and its entries are bare strings. Bending `RecentEntry` around a
`String` with no id would have coupled two things that only look alike.

## Edge cases

- **Missing file**: a fresh ledger, no warning. This is every first launch.
- **Missing directory**: `write` creates the parents. `resolved_app_data_dir` already does too, so this only matters
  for a data dir removed mid-session.
- **Unparseable file**: quarantined to `usage.json.broken` (one rotation kept), a warning logged, and this launch runs
  on an empty in-memory ledger. The next write starts a new file, so a corrupted ledger costs the history but leaves a
  copy on disk for anyone debugging it.
- **Unknown `_schemaVersion`**: same quarantine path. There's only ever been one version, so a migrator would be
  speculative; when v2 lands, the version check in `ledger::read` becomes a `match`.
- **Stale `usage.json.tmp`** (a write killed mid-flight): removed on the next read, so it can't shadow a later one.
- **Out-of-order or duplicated days** (a timezone move, a clock adjustment, a hand edit): the append guards on
  membership so it can't add a second entry for a day already present, and `launch_day_count` counts distinct days so a
  pre-existing duplicate can't inflate the gate. The file itself is left as it is.
- **A `launchDays` entry that isn't a date at all**: kept verbatim and counted. Nothing parses these back into dates,
  so a junk entry costs at most one spurious day on the gate; wiping the file to punish it would be the worse trade.

## Isolated data dirs

`record_launch` takes the dir that `config::resolved_app_data_dir` resolved, which honors `CMDR_DATA_DIR`. Every dev
run, every `pnpm dev --worktree <slug>`, and the E2E harness set that variable (matrix:
`docs/tooling/instance-isolation.md`), so a worktree session writes its own ledger and cannot touch the real one.
Verified by reading `config.rs:27`: the env branch wins over `app.path().app_data_dir()` whenever the variable is set
and non-empty.

## Privacy

The ledger is the only thing in the data dir that describes when someone uses the app, so it carries two hard rules,
both stated at the top of `mod.rs` and in `CLAUDE.md`:

- It never joins a crash report, an error-report bundle, or the feedback digest. Those bundles collect logs and
  diagnostics by explicit path; nothing globs the data dir today, and nothing should start.
- It never becomes a setting. `analytics/config_shape.rs` builds its PostHog snapshot by walking every bool and number
  setting, so a day count parked in `settings.json` would be uploaded without anyone deciding to upload it.

The privacy policy names it under "In the desktop app" (`apps/website/src/pages/privacy-policy.astro`).
