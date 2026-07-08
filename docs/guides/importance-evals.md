# Tuning the folder-importance scorer with evals

The importance scorer (`apps/desktop/src-tauri/src/importance/`) decides which folders matter, so expensive features
(the agent, media-ML enrichment, future cleanup and prefetch) can spend their effort on the right places. It ships with
default weights that are a starting point, not validated numbers. This guide is the measurement instrument for tuning
them: a suite of deterministic evals that turn "did that weight change help?" into a number you can watch move.

Everything here lives in `apps/desktop/src-tauri/src/importance/evals/`. You don't need to touch the scorer to use it.

## What the suite measures

A scenario is a set of folders, each with its derived signals (extension mix, recency, path class, project markers, and
so on), plus expectations about how they should rank once scored. Scoring a scenario runs the real scorer over every
folder and sorts them, then checks the expectations against that ranking. There are two tiers.

Hard constraints are ordering facts that must always hold: a `node_modules` scores zero, a project root outranks the
logs it generates, a cache never beats Documents. Each one is an ordinary test, so a violation fails the build. These
are the guardrails: they catch a change that breaks something fundamental.

Soft constraints are a larger set of desirable orderings: this folder belongs in the top three, that one belongs in the
bottom decile, this pair should rank in this order. The suite counts the satisfied fraction as a single quality score
per scenario, and averages those into one aggregate number. That aggregate is pinned to a floor. A change that drops the
aggregate below the floor fails; when a change improves it, you raise the floor by hand to lock the gain in. The floor
is a fixed line you move consciously, never a ratchet that quietly follows the score around.

The point of the soft score is that it moves smoothly. Turn a weight knob, and satisfying one more ordering nudges it up
a little. That makes `score(weights)` a fitness function: it's pure, fast, and side-effect-free, so you (or a future
grid-search) can call it in a loop and hill-climb toward better weights.

## How to run it

The evals are plain Rust tests, run through the normal checker:

```bash
pnpm check desktop-rust-tests
```

To run only the eval tests while iterating:

```bash
cd apps/desktop/src-tauri && cargo test --lib importance::evals
```

You'll see the hard-constraint test, the floor test, the per-scenario floor test, and the constraint-arithmetic and
anonymization unit tests. All green means the default weights still clear every hard constraint and the soft floor.

## How to read the score

The floor lives in `evals/tests.rs` as `SOFT_SCORE_FLOOR`. The floor test reports the aggregate soft score and compares
it. A `0.95` floor means "on average, at least 95% of soft orderings are satisfied." When you're tuning, the number you
watch is that aggregate: higher is better, `1.0` means every soft ordering holds.

A hard-constraint failure names the scenario, the constraint, and why it broke ("expected the project root above the
logs folder, but it isn't"), so you can see exactly what a change regressed.

## How to add a scenario

Scenarios live in `evals/scenarios.rs`. A small builder keeps them cheap to write: you describe each folder in a few
lines (some files with extensions, an age in days, a couple of flags), and the builder derives the signals through the
same classifiers production uses. So a folder you name `node_modules` floors exactly like the real thing, and a folder
under `Downloads` gets the same path-class prior it would in the app.

A new scenario is a builder chain: name it, set its home root and availability (local or listing-only for a network
volume), add folders, then add hard and soft constraints. Add it to the `all()` list at the top of the file. Two rules
keep a scenario honest, both enforced by tests: every constraint must name a folder that exists in the scenario, and
every scenario carries at least one hard and one soft constraint. Author the expectations to reflect rankings a correct
scorer already produces, so they act as a regression guard.

The shipped scenarios cover a developer home (a project versus its `node_modules`, caches, and logs), a media home (a
curated photo library versus a raw camera dump and screenshots), a downloads-heavy tree (a curated keep pile versus
installer and archive noise), and an SMB/NAS archive scored listing-only (no Spotlight, so it exercises weight
redistribution). Read them for the pattern.

## Tuning against your own folders: the full loop

Synthetic scenarios pin the scorer against homes we made up. To tune against reality, snapshot your own drive index into
the corpus. The loop is: snapshot, label, run, adjust, re-pin.

### 1. Snapshot your real index

The snapshot tool reads a drive index database (not the live filesystem, not `importance.db`) and exports an anonymized
scenario. It derives each folder's signals through the same code the scheduler uses, so a dump scores identically to how
the live volume would.

Your index databases live in the app data dir as `index-{volume_id}.db`. The local disk is `index-root.db`; a NAS share
is `index-smb-….db`. Point the tool at one:

```bash
DATADIR="$HOME/Library/Application Support/com.veszelovszki.cmdr"
CORPUS=apps/desktop/src-tauri/tests/importance-corpus

cargo run -p index-query --bin importance-snapshot -- \
  "$DATADIR/index-root.db" "$HOME" local root "$CORPUS"
```

The arguments are: the index database, your home (or mount) root, `local` or `listing-only` (use `listing-only` for a
NAS, which has no Spotlight), a short scenario name, and the output directory. The tool opens the database read-only, so
running it while the app is open is fine. For a NAS share whose mount root you don't have handy, any placeholder root
works: path-class anchors don't apply to a listing-only volume anyway.

Each snapshot writes two files into the corpus directory: `{name}.scenario.json` (the anonymized folders and their
signals) and `{name}.labels.json` (a template for step 2). Re-running keeps an existing labels file, so you won't lose
your marks.

### 2. Label your important folders

Open `{name}.labels.json`. It has an empty `important` list. Fill it with the folders that genuinely matter to you,
copying their (anonymized) paths from the matching `.scenario.json` dump. Each entry takes a path and an importance from
1 (most) to 3 (mildly):

```json
{
  "note": "...",
  "important": [
    { "path": "/home/Documents/dir-a298cb60", "importance": 1 },
    { "path": "/home/dir-11239b87/dir-96468a38", "importance": 2 }
  ]
}
```

The harness reads this back and turns each labeled folder into a soft "this folder ranks near the top" constraint,
tighter for higher importance. That's your personal ground truth: how well the weights rank the folders you care about.

The anonymized paths are placeholders, so finding your folders takes a little detective work. The structure is preserved
(the same real name always maps to the same placeholder), and classification-relevant names survive verbatim, so
`Documents`, `Downloads`, `node_modules`, and `.git` are readable landmarks. Navigate by those.

### 3. Run, adjust, re-pin

With the corpus in place, the eval suite auto-loads any labeled scenarios it finds and folds your labels into the soft
score. A dump only participates once it has labels: an unlabeled dump measures nothing, so the suite skips it without
even parsing it (which keeps things fast, since a real dump can be hundreds of megabytes). Run the suite (see above) and
read the aggregate. Then the tuning cycle is:

1. Turn a weight knob in `scorer/weights.rs`.
2. Run the eval suite.
3. Read the aggregate soft score. Did it go up?
4. When you're satisfied a change is a real improvement, raise `SOFT_SCORE_FLOOR` to the new level so the gain is locked
   in and can't silently regress.

Keep every hard constraint green throughout. If a weight change breaks a hard constraint, that's a real regression, not
a tuning tradeoff.

## What the anonymization keeps and strips (the privacy contract)

A snapshot is anonymized before anything is written, so no personal folder name leaves your machine. The scorer only
reads a folder's name through its classifiers, so every name that doesn't feed a classifier can become a placeholder
with zero effect on the score.

Kept verbatim, because the scorer's classification depends on them (and none of them are personal):

- Denylisted machine-output names (`node_modules`, `.git`, caches, build output), so they still floor.
- Any name starting with a dot, which drives hidden and system detection.
- The path-class anchors `Downloads`, `Desktop`, `Documents`, and `Library`, but only as direct children of your home
  root, where they classify a subtree.
- Project markers like `.git`, `.hg`, and `.svn`, which raise a project root.

Everything else, every personal folder name, becomes `dir-` followed by a short stable hash of the original. The same
real name always maps to the same placeholder within a dump, so the structure stays legible, but the original is
unrecoverable. Your home root itself becomes a fixed synthetic root (`/home` or `/volume`), so no username or mount path
leaks either.

What a dump keeps beyond names: the folder structure and depth, per-folder extension histograms, file counts, bucketed
modification times, and the hidden and system flags. What it never keeps: file contents, and any name that could carry
personal information. The result scores identically to your real tree while holding nothing personal.

Corpus dumps are never committed. The corpus directory is gitignored, and the eval suite is green with zero corpus files
present (which is what CI sees). Your dumps and labels are yours to keep locally.
