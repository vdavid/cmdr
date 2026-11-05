# Lane markers: details

## Why

Path filters gate each lane on the PR's whole delta against `main`. So once a branch touches Rust, every later push to
it reruns the Rust lanes, macOS (~34 min), and E2E (~21 min), even when the push only fixes a doc or a frontend string.
A lane's verdict depends only on the files it reads, so when those files are byte-identical to a set that already
passed, the rerun can't tell us anything new.

## The mechanism

1. **Hash** (`hash.go`): a lane's input set is every tracked file at the commit that matches any of its filters (the
   same matching dorny does, `match.go`), plus its definition: the workflow file it sits in, every local `uses:` target
   it reaches (composite actions as whole directories, reusable workflows as files, followed recursively), the same for
   every job it `needs` (but `changes`), and `alwaysInputs` (`.mise.toml`, `rust-toolchain.toml`, and `scripts/check/**`
   minus agent docs). The hash is SHA-256 over `hashVersion`, the lane id, and each input's mode, object ID, and path.
   Git object IDs are content hashes, so two commits hash a lane alike exactly when its input set and every input's
   bytes and mode match. It reads only the commit's tree (`git ls-tree`, `git cat-file`), never the working tree.
2. **Plan** (`changes` job, `lane-markers plan`): computes every lane's hash, lists the `lane-green-*` cache entries in
   the scopes this run trusts, and writes `lanes=<json>`: per lane `{hash, key, verified, ref}`. A failed listing warns
   and verifies nothing, so every uncertainty means "run".
3. **Skip**: a lane's `if:` adds `&& !fromJSON(needs.changes.outputs.lanes)['<id>'].verified` to its filter clause.
4. **Save**: a lane's last step saves an Actions cache entry keyed `lane-green-<lane>-<hash>` (the plan's `.key`). It's
   the last step, so it runs only when every step before it passed; a failed, cancelled, or skipped lane saves nothing.
5. **Verify** (`CI OK`, `lane-markers verify`): recomputes every lane's hash from its own checkout and, for each lane
   that skipped while a filter in its `if:` was on, requires all of: a run that trusts markers, a plan entry with
   `verified`, the plan's key equal to the recomputed one, and that key present in a trusted scope, listed again. Any
   other skip of a changed lane is red: a wiring slip, a skipped `needs` job, or a plan that lied. A `run_all` run with
   any skipped lane is red.

## Trust argument

The claim a verified skip makes: "this lane passed, in a run we trust, on exactly these inputs". Each part:

- **Exactly these inputs**: the key is a content hash of the input set, recomputed independently by `CI OK`. Same key
  means same bytes, modes, and file set (a deleted, added, or renamed input changes it).
- **Passed**: only the lane's own last step saves, and only after every earlier step succeeded. Cache keys are immutable
  (first save wins), so nothing overwrites a marker with a different meaning.
- **A run we trust**: the scope (`ref`) of a cache entry is set by GitHub from the run that saved it, and the plan
  accepts only two scopes:
  - `refs/heads/main`: only runs on `main` (landed code, running `main`'s workflows) save there.
  - The PR's own `refs/pull/<n>/merge`, and only when the PR's head is in this repo. A fork PR's run executes the fork's
    workflow, which could save a marker for a hash it computed offline, then push a clean workflow plus untested code
    whose lane would skip on it. GitHub's cache isolation keeps that marker out of every other scope, and the fork rule
    keeps it out of the fork's own later runs. Same-repo authors already control the workflow; they're trusted.
  - Never another PR's, a release branch's, or any other ref, even though the cache API lists them all:
    `parseMarkerList` and `decide` compare the ref by equality.
- **Who trusts at all**: only `pull_request` runs. A push to `main`, a dispatch (the release gate's `run_all`), and a
  schedule ignore markers and run every lane their filters select, then save fresh markers (into `main`'s scope, which
  seeds every PR).

## Store choice: the Actions cache

- **Actions cache (chosen)**: zero infrastructure, writable from fork PRs with a read-only token (so no secret or write
  permission enters PR runs), scoped by GitHub per ref with `main` readable by every PR, immutable keys, and a built-in
  TTL (entries unused for 7 days go). Listing needs `actions: read`. A marker is one tiny file, so it costs nothing
  against the 10 GB budget. Eviction only ever makes a lane run.
- **Commit statuses or check runs**: attached to a SHA, so a hash can't be looked up across commits, and a fork PR's
  token can't write them anyway.
- **Workflow artifacts**: listable repo-wide by name, including artifacts uploaded by fork PR runs, so trust would rest
  on re-deriving each uploader's event, repo, and job result. More code to get wrong for the same answer.
- **Git refs or notes**: need `contents: write` in PR runs, which Scorecard's Token-Permissions and the fork model both
  rule out.
- **An external store** (KV, D1): needs a secret that fork runs don't get and that every PR run would then hold.

Caveats of the choice: markers are never restored (only listed), so under size pressure LRU evicts them first (they read
as least recently used); that costs reruns, never correctness. `prune-superseded-caches` prunes only the families its
`case` names (`v0-rust-*`, `*-docker-e2e-*`, `light-*`, `pnpm-store-*`), so it leaves `lane-green-*` alone; keep it that
way (every marker key is its own family's only member, so pruning would delete live markers).

## The one risk: a filter missing an input

A marker is exactly as good as the lane's input set. If a lane reads a file no filter (and no definition rule) covers,
today's path filter already skips the lane when only that file changes. Markers widen that gap: today a push that
touches any covered input reruns the lane against everything current, so an uncovered input gets tested by accident;
with markers, the lane reruns only when a covered input differs from a passed set. Guards:

- **Full runs ignore markers**: the release gate's `run_all` (every release), and every `main` push that isn't a
  verified land. A filter that misses an input still gets caught there, at worst one release later. A scheduled
  `run_all` would tighten that window further.
- **Over-inclusion by construction**: whole workflow files, whole action directories, the jobs a lane `needs`, the
  toolchain pins, and the check runner are always in. A filter edit changes the hashes the way it changes dorny's
  verdicts.
- **The 7-day TTL** bounds how long one passed set can stand in for a rerun.
- **`ci-coverage`** keeps every concrete filter path existing (rule 3), so a renamed input can't silently drop out.
- **Unsupported glob syntax refuses to hash** (`compileGlob`), so a new pattern can't match differently from dorny.
- What a commit can't capture (runner image updates, network-fetched tools outside the lockfiles, flaky tests) is the
  same exposure as today's path-filter skip, bounded by the same full runs.

## Matching fidelity

`match.go` mirrors the filter source of dorny/paths-filter v4.0.3 (its filter.ts, read 2026-11-05): picomatch with
`{dot: true}`, a leading `!` inverting the matcher, and the three quantifiers (`some`, `every`, `some-with-excludes`,
where an exclusion is final and excludes alone include nothing). The glob subset (`**` segments, `*`, `?`, literals, and
plain character classes like `[Dd]`, which the `scorecard` filter spells case-insensitive names with) is pinned to
picomatch 2.3.2's own verdicts in `TestGlobMatchesPicomatch` and `TestClassGlobsMatchPicomatch` (dorny depends on
`picomatch ^2.3.1`). Two picomatch quirks, found by probing it (`node`, picomatch 2.3.2, 2026-11-05):

- A trailing `/**` matches the bare parent (`crates/**` matches `crates`) except after a segment ending in `*`
  (`*[Ll][Ii][Cc][Ee][Nn][CcSs][Ee]*/**` doesn't match `LICENSE`). `matchSegsAfter` mirrors it.
- A class also matches its own literal text (`[a]b` matches `[a]b`). Not mirrored: no tracked path spells out one of our
  classes. Status-scoped rules (`added|modified: ...`) are refused: a tree has no change status. dorny matches changed
  paths and we match every tracked path, which is the same per-path predicate.

## Wiring (per lane, and in `CI OK`)

All six path-filtered lanes in `ci.yml` follow this (`docs/tooling/ci.md` § "Lane markers (skip a lane whose exact
inputs already passed)" lists them). `changes` and `CI OK` get the built binary from `.github/actions/lane-markers-tool`
(the Actions cache, keyed on its sources and the Go pin; a miss installs Go with mise, builds, and saves), which saves
~20 s in each of those jobs, both on every run's critical path. The save is the `lane-marker` composite action
(`.github/actions/lane-marker`), which refuses a key without the `lane-green-` prefix; a multi-job lane file (Svelte,
website) saves from a `marker` job that `needs` every other job, so one green job can't vouch for another.

`changes` job:

- `permissions:` add `actions: read`; `outputs:` add `lanes: ${{ steps.lanes.outputs.lanes }}`.
- After the filter step, with Go available (mise or a SHA-pinned `setup-go` on `scripts/lane-markers/go.mod`), a step
  with `id: lanes`, env `GH_TOKEN`, `EVENT: ${{ github.event_name }}`, `REF: ${{ github.ref }}`,
  `REPO: ${{ github.repository }}`, `HEAD_REPO: ${{ github.event.pull_request.head.repo.full_name }}`, and
  `RUN_ALL: ${{ inputs.run_all == true }}`, running
  `go -C scripts/lane-markers run . plan -event "$EVENT" -ref "$REF" -repo "$REPO" -head-repo "$HEAD_REPO" -run-all="$RUN_ALL"`.
  It must run on every event (a lane's `if:` reads its output; a missing output fails the expression).

Each lane job:

- `if:` becomes `inputs.run_all || (<its filter clause> && !fromJSON(needs.changes.outputs.lanes)['<id>'].verified)`.
- Last steps: write any small file (say `echo "$GITHUB_RUN_ID" > "$RUNNER_TEMP/lane-green"`), then `actions/cache/save`
  (the SHA-pinned one `ci.yml` already uses) with that `path` and
  `key: ${{ fromJSON(needs.changes.outputs.lanes)['<id>'].key }}`. No `if:` on them, and nothing after them.
- A lane calling a reusable `<lane>-lane.yml` passes the key as an input (`marker-key:`); the reusable job's last steps
  save it. The calling job then reads `.key`, which is what makes it the lane.
- A prerequisite job (`e2e-base-image`, `website-demo`) follows its lane's verdict with the same
  `!fromJSON(...)['<lane>'].verified` clause and saves nothing. Left on its own filter, it would build for a lane that
  skips. Made a lane of its own, it could verify while its lane doesn't, and the lane would then cascade-skip on the
  skipped `needs`, which `verify` reports red on every push until the inputs change. The lane's hash already covers the
  jobs it `needs`.

`CI OK`:

- Add a checkout, Go, `actions: read`, and after the existing jq gate a step running
  `go -C scripts/lane-markers run . verify -needs "$NEEDS" <the same context flags>` with `NEEDS: ${{ toJSON(needs) }}`.
  The existing gate still judges every lane that ran.
- `ci-coverage` rule 5 reads a lane's `if:` folded over several lines (`if: >-`), which the longer gate needs.

Off CI, `go run . hash -files` lists each lane's key and input files at `HEAD`, which is how to answer "why did this
lane run (or not)".
