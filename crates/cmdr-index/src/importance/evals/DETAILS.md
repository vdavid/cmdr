# Importance evals — details

The measurement instrument for tuning `Weights`: it turns "did this weight change help?" into a number instead of a
vibe. Read this before any non-trivial work here: editing, planning, reorganizing, or advising. The David-facing how-to
(running the suite, reading the score, adding a scenario, the snapshot/label/tune loop, the privacy contract) is
`docs/guides/importance-evals.md`; this doc is the design.

The scorer ships deliberately-unvalidated default weights (`../scorer/DETAILS.md` § Tunable weights), which is the
plan's open-question 1. This module is what closes it.

## The two tiers, and the floor

- **Hard constraints** are ordering facts that must ALWAYS hold. `tests.rs` asserts each as an ordinary `#[test]`, so a
  violation fails CI — a regression that lets `node_modules` climb out of the bottom decile breaks the build.
- **Soft constraints** are a larger set of desirable orderings. `score_scenario` returns the satisfied fraction as a
  scalar quality score, and `aggregate_score` averages across scenarios. The aggregate is pinned to a FIXED floor
  constant (`SOFT_SCORE_FLOOR` in `tests.rs`, currently `0.95`), consciously raised when tuning improves quality — never
  a self-updating ratchet, because a ratchet would let a slow slide re-baseline itself.

Both tiers speak the same `Constraint` vocabulary; only the treatment of a violation differs.

## The constraint vocabulary (`constraints.rs`)

- `Above { above, below }` — the workhorse pairwise ordering fact ("the project root outranks its `node_modules`").
- `TopN { path, n }` — the folder lands within the top `n` (0-based rank `< n`).
- `BottomDecile { path }` — the canonical "machine output is ignorable" expectation for caches, logs, `node_modules`.
- `DecileAtMost { path, at_most }` — a softer "belongs near the top" than `TopN` (decile 1 is best).
- `ScoreAtMost { path, max }` — pins an absolute value pure ordering can't, notably "a floored folder really scored
  `0.0`".

`Constraint::evaluate` returns `ConstraintOutcome::{Satisfied, Violated(why), Unknown(why)}`. `Unknown` (a constraint
naming a path that isn't in the scenario) counts as unsatisfied for scoring but is reported distinctly, so a mistyped
scenario path is obvious rather than silently dragging the score.

**The `Ranking` tie rule.** `Ranking::from_scores` sorts score DESC then path ASC — the SAME stable order the read API's
`top_n` uses (`read::read_ordered`'s `ORDER BY`), so a scenario ranks folders exactly as a live consumer would. Ties
therefore resolve deterministically by path, never by input order. Owning "rank", "top N", and "decile" here keeps those
definitions single-sourced and unit-tested in one place.

## The fitness function (`mod.rs`)

`rank_scenario(scenario, weights)` scores every folder through the pure scorer under the scenario's availability mask
and returns a `Ranking`. `score_scenario(scenario, weights) -> f64` is the pure, fast fitness function a tuner
optimizes: nothing about it touches I/O, a clock (the scenario carries its own `now_secs`), or global state, so a grid
search or hill-climb can call it in a loop. `aggregate_score(scenarios, weights)` averages per-scenario scores into one
number.

## The scenario format (`scenario.rs`)

A `Scenario` is a home root, a `now_secs`, an `Availability`, a list of `ScenarioFolder`s (path + derived
`FolderSignals`), and the two constraint tiers. It is deliberately NOT a synthetic filesystem: the pure scorer only
needs signals, and signals are exactly what a corpus tool can export from a real index. So one type serves both sources
— hand-authored synthetic trees and anonymized real dumps load into the same `Scenario` and score through the same path
— and the privacy contract stays simple, because a `FolderSignals` holds counts, flags, and timestamps, never file
contents.

`Availability` is a named enum (`Local` / `ListingOnly`) rather than the raw `SignalSet` bools, so a scenario file reads
clearly and a new backend kind stays a one-line addition. `ListingOnly` is the SMB/NAS degradation: Spotlight
unavailable, so `last_used`'s weight redistributes.

## The committed synthetic scenarios (`scenarios.rs`)

All committed scenarios score against one fixed `NOW` so recency is deterministic. Each is authored as a list of terse
`FolderSpec`s, and the builder derives every folder's `FolderSignals` through the PRODUCTION classifiers, so a synthetic
folder classifies exactly as the live scheduler would.

- **A developer's home** — the archetypal case, and the one the scorer most has to get right: an active `.git` project
  root with mixed source must dominate, while the machine output it generates (`node_modules`, build caches, a logs
  monoculture) must sink to the bottom, with Documents and a mixed Downloads in between.
- **A media home** — a curated photo library and an active editing project must beat a raw camera dump (a
  single-extension monoculture), an app-generated screenshots pile, and a thumbnail cache. Exercises "diverse user
  content over single-kind machine dumps" with no project marker in play.
- **A downloads-heavy tree** — the folder the person actually curates (mixed, revisited) must surface over disposable
  installer piles and an unpacked archive's internal directories.
- **An SMB/NAS archive** — a backups tree, a photo archive, and a media library plus machine-output noise, scored
  `ListingOnly`. It exercises the redistribution path: the ranking must still separate real content from noise using
  only the listing signals plus Cmdr-navigation visits.

## The corpus (`corpus.rs`)

Synthetic scenarios pin the scorer against homes we made up. A corpus captures David's REAL folder structure so tuning
optimizes against reality. Real folder names are PII, so a snapshot is anonymized before it is ever written.

**Signal derivation reuses production.** `snapshot_index_to_scenario` calls `scheduler::walk_index_folders` +
`signals::signals_for_dir`, so a dumped scenario's signals match what the live scheduler computes.

**Anonymization (the privacy crux).** The scorer reads a folder name ONLY through the classifiers, so every name that
doesn't feed one becomes a stable `dir-<8 hex>` placeholder with zero effect on the score (the same real name maps to
the same placeholder across a dump, so structure stays legible, but the original is unrecoverable). Kept verbatim:

- **denylist hits** (`node_modules`, `.git`, build output — anything `classify::is_denylisted`), because the denylist
  floors them;
- **dot-prefixed names**, because hidden/system detection keys off the leading `.`;
- **path-class anchors that are direct children of the home root** (`Downloads`, `Desktop`, `Documents`, `Library`),
  because `classify::path_class` matches those by name (`PATH_CLASS_ANCHORS` is kept in sync with it);
- **project markers** (`.git`, `.hg`, `.svn` as directory names), because they raise a project root.

The home root itself becomes a synthetic `/home` or `/volume`. Everything kept is structural and non-personal by
construction. `corpus/tests.rs` pins the rules.

**Never committed.** `snapshot_index_to_scenario` writes a `.scenario.json` plus a `.labels.json` template into the
GITIGNORED corpus dir (`CORPUS_DIR_REL` = `apps/desktop/src-tauri/tests/importance-corpus`). `load_corpus_scenarios`
returns an empty vec when the dir is absent, and `tests.rs` auto-includes whatever it finds, so the committed suite is
fully green with zero corpus files present (which is CI's situation).

## The `importance-snapshot` bin

`crates/index-query`'s `importance-snapshot` wraps the corpus tool: read a real `index-{volume_id}.db` READ-ONLY, derive
signals, anonymize every folder name, and write the scenario plus labels template into the corpus dir. David then labels
the template with his own expectations, and the labeled scenario joins the suite locally. Running order and the labeling
conventions: `docs/guides/importance-evals.md`.
