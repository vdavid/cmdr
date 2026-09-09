# Usage

The launch-day ledger: which local calendar days Cmdr has been opened on. One durable file,
`usage.json` in the app data dir, holding `{ "_schemaVersion": 1, "launchDays": ["2026-09-09", …] }`.
It's the gate for usage-gated hints, the first being the "add Cmdr to your Dock" nudge.

## Module map

- **`mod.rs`**: the two public calls, `record_launch(data_dir)` (once per launch, from `lib.rs` setup) and
  `launch_day_count(data_dir)` (the read seam behind `commands::usage::get_launch_day_count`).
- **`ledger.rs`**: the `_schemaVersion` envelope, the pure `appended` day-append, the durable temp+rename write, and
  the `.broken` quarantine.

## Must-knows

- ❌ **The ledger never leaves the device.** It must not reach a crash report, an error-report bundle, or the feedback
  digest. The privacy policy discloses it as local-only, so an accidental upload would break a stated promise.
- ❌ **It must not become a setting.** `analytics/config_shape.rs` auto-ships every bool and number setting to PostHog,
  so a day count stored there would silently turn into telemetry. Its own file is what keeps it out.
- **Local calendar day, never UTC.** "Your third day using Cmdr" is a claim about the person's own calendar, and a
  late-evening session in Stockholm must not count as tomorrow.
- **The append guards on MEMBERSHIP, never on the last element.** A timezone move or a clock adjustment can leave the
  days out of order, and a tail comparison would then record a day the ledger already holds. Existing entries are kept
  verbatim (no sort, no dedupe), so `launch_day_count` counts DISTINCT days rather than trusting the file.
- **A file we can't read is quarantined, not wiped.** A parse failure or an unknown `_schemaVersion` renames it
  `.broken` (one rotation kept) and this launch runs on an empty in-memory ledger, matching `recents/persistence.rs`.
- **Every failure is a logged warning.** An unwritable ledger costs a hint, never a launch, and never an error a user
  sees.
- **Unbounded growth is intended.** A decade of daily use is a few tens of KB; a cap would silently break "how long
  have you been here" questions we haven't asked yet.

Why it's synchronous, why it isn't a `RecentsFile`, and the isolated-data-dir story: `DETAILS.md`.
