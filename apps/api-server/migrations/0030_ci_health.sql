-- The last state `ci-health.ts` saw for each CI signal (`main`, `nightly`, `upkeep`, `slow-checks` read `green` or
-- `red`; `nightly-ran` and `main-ran` read `ok` or `silent`; `github` reads `ok` or `unreachable`), so the three-hour cron
-- writes an ops event only when one changes. No personal data: a signal name, a state, and two times.
CREATE TABLE ci_health (
    signal TEXT PRIMARY KEY,
    state TEXT NOT NULL,
    changed_at TEXT NOT NULL,
    checked_at TEXT NOT NULL
);
