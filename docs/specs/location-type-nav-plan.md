# Plan: make `(volumeId, path)` a first-class `Location`, kill bare-path navigation

## Problem (the bug that started this)

Running a search while a pane is on an SMB/NAS volume, then pressing Enter on a result whose path is local (e.g.
`/Users/veszelovszki/Library/Application Support/...`), produces an SMB protocol error
(`STATUS_OBJECT_PATH_NOT_FOUND during Create`). The pane header also goes inconsistent: the tab label reads `naspi` (the
NAS) while the volume selector reads `Macintosh HD`.

Root cause: navigation can carry a **path without a volume**. `navigate()`'s in-place arm loads the target path on the
pane's _current_ volume unless the pane happens to be on the `search-results` virtual volume (the only case the existing
`isCrossVolumeNavigation` gate catches). So a local path gets listed over the SMB connection. The two header halves
disagree because the volume never switched: the tab label comes from `tab.path` (still the NAS root, since the failed
listing never committed), while the volume selector resolves the _in-flight_ `currentPath` to its real volume.

The backend is **not** at fault: `listDirectoryStart(volumeId, path)` faithfully routed the local path to the SMB volume
because the frontend told it to. This is a frontend-internal volumeId/path mismatch.

## The fix in one sentence

Give navigation two explicit destination shapes — **`{ location: Location }`** (go somewhere; `volumeId` is mandatory)
and the existing **`{ volumeId, path }`** (deliberately (re)select a volume) — delete the bare-`{ path }` hole, and
resolve the few genuine bare strings (⌘G input, MCP `nav_to_path`, search-result activation, downloads reveal) into a
`Location` once, at the edge, erroring if they can't be resolved.

## Why this shape (intent — read before changing anything)

- **Two operations, two names.** "Go to a location" and "select/activate a volume" are genuinely different intents that
  happen to share a `(volumeId, path)` payload. The current API conflates them through one arm whose `volumeId` is
  optional: present ⇒ volume-(re)select (optimistic commit, respects `pushHistory`/terminal recovery), absent ⇒ in-place
  path move. Naming them separately is what makes the bug unrepresentable **without** breaking the volume-(re)select
  callers that legitimately pass the _current_ volume id.
- **CRITICAL correction over the first draft.** An earlier version of this plan proposed flipping the single arm's
  trigger to `volumeId === currentVolumeId ? in-place : switch`. That is WRONG: several callers pass
  `volumeId === current` and depend on the _switch_ arm — network-restore-on-cancel (`DualPaneExplorer.svelte:633`),
  cancel walk-up (`:665`), retry-unreachable (`:710`), `selectVolumeByIndex` re-selecting the current volume (`:1442`),
  `onVolumeChange` with a favorite on the current volume (`:1894`), and `mirrorLocalStateToPane` (`:1597-1608`, which
  _already_ hand-splits same-vs-cross precisely because `{ volumeId: current }` hits the switch arm today). Routing
  those into the in-place arm would hit its first statement — the on-network refusal (`navigate.ts:532`) — or silently
  drop `pushHistory`/terminal semantics. So we do NOT flip the existing arm; we ADD a `{ location }` arm and leave
  `{ volumeId, path }` as the always-switch volume-(re)select arm.
- **`navigate()` gets a clean two-way split, not a heuristic.**
  - `{ volumeId, path }` → switch arm, always (today's behavior, untouched).
  - `{ location }` → if `location.volumeId === currentVolumeId` → in-place arm (commit-on-listing, pinned-tab fork at
    `commitPathFromListing`, `push-path`); else → switch arm (`commitVolumeSwitch`). The only callers of `{ location }`
    are genuine path navigations, so the `=== current` test is safe _here_ (it is not safe for the volume-(re)select
    callers, which is exactly why they keep their own arm).
- **Resolve where the context is.** A bare string legitimately enters navigation at four edges: ⌘G (user-typed), MCP
  `nav_to_path` (external/agent), search-result activation (a real path from the index), and downloads reveal (⌘J). Each
  resolves to a `Location` before navigating. Failure becomes honest UX (a friendly toast / typed MCP refusal) instead
  of today's cryptic `STATUS_OBJECT_PATH_NOT_FOUND` (design principle 3).
- **One name, both sides of the IPC boundary.** `bindings.ts` is Tauri-Specta-generated from Rust, so `Location` is
  defined once in Rust and the TS type is generated. FE navigation/tab/history types compose the same shape. Single
  source of truth (smart-backend/thin-frontend, single-source-doc principles).

## Key facts established during investigation (so the implementer doesn't re-derive them)

- **The IPC boundary is already volume-safe.** `listDirectoryStart(volumeId, path)` →
  `commands.listDirectoryStartStreaming(volumeId, ...)` (`file-listing.ts:49-59`); `FilePane.loadDirectory` passes the
  pane's `volumeId` prop. No backend listing change is needed for the fix.
- **`resolvePathVolume(path)`** (`storage.ts:75` → Rust `resolve_path_volume`, `commands/volumes.rs:60`) returns
  `{ volume: VolumeInfo | null, timed_out }`. It takes **no `AppHandle`** — it calls `volumes::resolve_path_volume_fast`
  (statfs) directly. For local paths it returns the correct local id (`"root"` or a mounted id). **Caveats:** for
  `smb://` paths it returns the _virtual_ `"network"` id, not a specific SMB connection (`commands/volumes.rs:71-89`);
  for a path on a currently-unmounted/indexed volume it returns `volume: null` or `timed_out`.
- **`searchableFolder` carries no volumeId today** (`SearchDialog.svelte:124`); the index can return paths outside the
  pane's volume. So search **must** resolve each activated result's path — it cannot assume the pane's volume.
- **Do NOT touch `resolve_go_to_path`.** It has TWO FE consumers — the jump (`go-to-path.ts:55`) AND a debounced
  per-keystroke preview (`GoToPathDialog.svelte:94`, which only wants the `nearestAncestor` hint). Changing its return
  type breaks both (so M1 wouldn't stay green), and folding a volume `statfs` into it would add a discarded
  `blocking_with_timeout` (up to 2 s on a hung mount) to every keystroke — a P3/P5 regression. ⌘G instead resolves its
  volume on the FE at **jump time** (in `go-to-path.ts`), exactly like the other three edges. `resolve()` stays pure and
  its ~8 unit tests (`mod.rs:189-340`) stay untouched.
- **One resolver for all four edges.** Add a dedicated AppHandle-free command
  `resolve_location(path) -> { location: Location | null, timed_out }`. It MUST replicate the FULL `resolve_path_volume`
  logic, not just the fast helper: the `mtp://` / `smb://` protocol dispatch lives in the command body
  (`commands/volumes.rs:62-89`, smb → virtual `network` id), and only the local-FS branch is `resolve_path_volume_fast`
  (`volumes/mod.rs:273`). Calling `_fast` alone would return `null` for Cmdr's virtual `smb://`/`mtp://` paths (a
  regression, and it would break the documented MCP-narrowing behavior that relies on `smb://` → `network`). Cleanest:
  factor the `resolve_path_volume` command body into a shared `fn → (Option<VolumeInfo>, timed_out)` and have BOTH
  `resolve_path_volume` and `resolve_location` call it (the latter mapping `VolumeInfo → Location`). Wrap with
  `blocking_with_timeout_flag` (NOT bare `blocking_with_timeout`) so `timed_out` is actually populated — matches
  `resolve_path_volume` (`commands/volumes.rs:92-98`). This is the `Location` specta-export vehicle and the single
  backend the FE `resolveLocation` wraps. `PathVolumeResolution` does NOT change.
- **`NavigateCommit.volumeId` MUST stay optional** (`navigate.ts:160-175`). It is the internal commit shape; the
  in-place landing `commitPathFromListing` calls `commit(deps, { pane, path, history: 'push-path' })` with no volumeId
  (`navigate.ts:701`), and `commit()` keys on `if (c.volumeId !== undefined) setPaneVolumeId(...)` (`:324`). Only
  `NavigateTo`'s arms gain a required `volumeId` (via the new `{ location }` arm and by making the existing
  `{ volumeId, path }` non-optional). Do NOT touch `NavigateCommit`.
- **`Location` must be referenced by a registered command signature** or tauri-specta won't emit it to `bindings.ts`.
  The new `resolve_location` command's return type is that vehicle.
- **Existing (volumeId, path) pairs to unify** (none named today): FE `HistoryEntry` (`navigation-history.ts:30`, also
  `networkHost?` — keep that OUT of `Location`; `HistoryEntry = Location & { networkHost? }`), `TabState`/`PersistedTab`
  (`tab-types.ts:16-40`, flat `{ volumeId, path }` — composing `Location` keeps field names, so `app-status.json`
  round-trips; `HistoryEntry` is NOT persisted, zero serialization impact), `LastUsedPathRecord` (`navigate.ts:178`),
  `getPaneLocation()`'s return (`DualPaneExplorer.svelte:1403`, also has `volumePath` — compose where it reduces dup,
  don't force it). BE: `CachedListing` (internal) + a `(String, PathBuf)` tuple. `LocationInfo` (`volumes/mod.rs:40`) is
  a volume/favorite descriptor, NOT a (volume, path) pair — don't conflate or rename it.
- **The bare-`{ path }` compile-error set** (callers to migrate to `{ location }` when `volumeId` becomes required):
  `handleSearchNavigate` (`+page.svelte:657`), MCP `nav_to_path` (`mcp-listeners.ts:284`), `navigateToDirInPane` /
  `navigateToFileInPane` (`navigate-and-select.ts:49,71`, used by ⌘G `go-to-path.ts:68,72,76` and ⌘J go-to-latest via
  `revealFileInBestPane`/`navigateToDirInBestPane`), the in-place async-resolve block inside `navigate()` itself
  (`navigate.ts:538-562`), and **`mirrorLocalStateToPane`'s bare `{ path }` at `DualPaneExplorer.svelte:1607`** (knows
  its volume id already — classify it: it's a "go to location" so it becomes `{ location }`).
- **FilePane-internal bare paths are legitimately same-volume** and stay bare: `loadDirectory`, `navigateToPath`,
  breadcrumb-segment clicks, normal folder-Enter (`handleNavigate` on a real listing). They use the pane's current
  `volumeId` because entering a folder / clicking an ancestor is inherently same-volume, and they bypass `navigate()` by
  design (P2 perf; `navigate.ts:568-575`). The ONE exception is `handleNavigate` on a **search-results pane** opening a
  real entry — see M2 (both the `isDirectory` branch at `FilePane.svelte:1864` AND the `redirectToPath`
  worktree/submodule branch at `:1847`).

## What actually gets deleted vs kept (the review corrected the first draft's "net deletion" claims)

- **DELETE**: the bare-`{ path }` arm (the hole); the in-place async-resolve block + its `isCrossVolumeNavigation` use
  (`navigate.ts:538-562`); `deps.resolveVolume` from `NavigateDeps` (`navigate.ts:212`) and its DPE wiring
  (`DualPaneExplorer.svelte:375`) — it is used ONLY by that deleted block; `snapshot-pane-navigation.ts` +
  `isCrossVolumeNavigation` (its two responsibilities move: detection → a capability check; routing → `navigate()`);
  `FilePane.switchVolumeForRealPath` (replaced by resolve-Location + routing the snapshot-row activation through
  `navigate()` via a new location callback).
- **KEEP, comment-only**: `has-parent.ts`. It has **no** code coupling to `isCrossVolumeNavigation` — only a stale prose
  comment (`has-parent.ts:17` asserts "stays coupled to `isCrossVolumeNavigation`"). `computeHasParent` keys on the
  `hasParentRow` capability (`:42-43`), NOT a `volumeId` string (invariant A6 forbids the string compare). REMOVE the
  now-false coupling sentence (don't reword it around a deleted symbol); change no code; do NOT introduce a
  `volumeId === 'search-results'` compare.
- **Snapshot-pane detection** (where FilePane decides "I'm on the results pane, this is a real entry → switch"): use a
  **capability** (`volumeKindOf(volumeId) === 'search-results'` / the search-results `VolumeKind`), NOT a raw id string,
  per A6. This replaces `isCrossVolumeNavigation`'s string compare with the sanctioned classifier.

## Naming

- Rust: `pub struct Location { pub volume_id: String, pub path: String }` with
  `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]`, `#[serde(rename_all = "camelCase")]`.
  Generates `Location = { volumeId: string; path: string }` in `bindings.ts`. `getPaneLocation()` already gestures at
  the name; `LocationInfo` (the volume descriptor) stays distinct.

---

## Milestone 1 — Foundations: the shared `Location` type + edge resolvers (additive, ends green)

### 1a. Rust `Location` type + the `resolve_location` command + regenerate bindings

- Define `Location` (see Naming) in a discoverable shared module (e.g. `src-tauri/src/location.rs`, or alongside
  `PathVolumeResolution` in `commands/volumes.rs`).
- Add command `resolve_location(path: String) -> ResolveLocationResult` where
  `ResolveLocationResult { location: Option<Location>, timed_out: bool }`. Implement via the shared
  `resolve_path_volume` body (full protocol dispatch + fast local helper — see Key facts), under
  `blocking_with_timeout_flag` (no `AppHandle` needed). Register it (custom invoke-handler commands aren't gated by
  `capabilities/*.json`, so no capability entry needed).
- `pnpm bindings:regen`; confirm `Location` + `ResolveLocationResult` land in `bindings.ts` (generated;
  `file-length`-exempt).
- Leave `resolve_go_to_path` / `GoToPathResolution` / `resolve()` and their tests completely untouched.

### 1b. FE `resolveLocation` edge helper

- Add a thin FE helper (e.g. `lib/file-explorer/navigation/resolve-location.ts`) wrapping the new command:
  `resolveLocation(path): Promise<{ ok: true; location: Location } | { ok: false; reason }>`. The SINGLE FE primitive
  for all four edges (search, ⌘G jump, downloads reveal, MCP, snapshot-row). Map `location: null` and `timed_out` to
  `{ ok: false }`. Honor the two-layer timeout pattern.

### Tests (M1)

- Rust (TDD red→green): `resolve_location` returns the correct `volumeId` for a local dir and a local file over a
  `tempfile::tempdir()` (root volume suffices); a path resolving to no volume ⇒ `location: None`.
- FE unit: `resolveLocation` maps success → `{ ok: true }`, null location → `{ ok: false }`, timeout → `{ ok: false }`.

### Docs (M1)

- Document `resolve_location` + `Location` near `commands/volumes.rs` / its module `DETAILS.md` as the canonical
  path→volume resolver for navigation edges.

### Checks (M1)

- `pnpm check rust`, then full `pnpm check` (bindings, ipc-enum-camelcase, svelte unit). M1 is purely additive — a new
  command + a new FE helper, zero existing consumers touched — so it ends green.

---

## Milestone 2 — Core: add `{ location }`, make `volumeId` required, migrate bare-path callers, delete the hole

One bounded scope; one agent owns it. **Stage it to stay green throughout** (no multi-file red window): (1) add the
`{ location }` arm while temporarily KEEPING the bare `{ path }` arm — additive, compiles; (2) migrate each caller to
`{ location }`, compiling green after each; (3) as the final step, delete the bare `{ path }` arm and make
`{ volumeId, path }`'s `volumeId` required. Same endpoint, every intermediate commit builds.

### 2a. Intent type

- `NavigateTo` becomes:
  ```
  | { location: Location }              // go to a location (NEW; volumeId required)
  | { volumeId: string; path: string } // select/activate a volume (volumeId now REQUIRED, was optional)
  | { history: 'back' | 'forward' | 'parent' }
  | { snapshot: string }
  ```
- Do NOT change `NavigateCommit.volumeId` (stays optional — see Key facts).
- Compose `Location` into `HistoryEntry` (`Location & { networkHost? }`), `TabState`/`PersistedTab`,
  `LastUsedPathRecord`, and `getPaneLocation()`'s return where it cleanly reduces duplication and preserves field names.
  Verify the persistence round-trip test stays green.

### 2b. `navigate()` arm routing

- `{ volumeId, path }` → switch arm, unchanged (all the DPE volume-(re)select callers keep working as-is).
- `{ location }` → `location.volumeId === currentVolumeId` ? in-place arm : switch arm.
- Delete the in-place async-resolve block (`navigate.ts:538-562`) + its `isCrossVolumeNavigation` import; delete
  `deps.resolveVolume` from `NavigateDeps` + DPE wiring; delete `snapshot-pane-navigation.ts`. Keep the
  `currentVolumeId === 'network'` and MTP refusals in the in-place arm and re-check their order now that the
  cross-volume branch is gone.

### 2c. Migrate the bare-`{ path }` callers (the compile-error set)

- **handleSearchNavigate** (`+page.svelte:650-664`): this navigates to the result's PARENT dir then `moveCursor` onto
  the file — so resolve the **parentDir**, not the file path. `await resolveLocation(parentDir)`; ok →
  `navigate({ to: { location }, ... })` then the existing `moveCursor(fileName)` follow-up (the switch arm's
  `settled`-before-listing is bridged by `moveCursor`'s `whenLoadSettles`); failure → friendly toast, no navigation. It
  is now identical to `navigateToFileInPane` (resolve-parentDir → navigate → moveCursor); prefer delegating to that
  helper once it takes a `Location`.
- **MCP `nav_to_path`** (`mcp-listeners.ts:284`): `await resolveLocation(path)` before navigating; failure →
  `mcp-response { ok: false, error }` with a typed message (add a `NavigateRefusal` kind or handle in the adapter; keep
  existing refusal strings byte-for-byte, new message additive). **Behavior change to state + test:** the on-network
  refusal _trigger_ narrows — today a bare `{ path }` from a pane on `network` refuses for ANY target
  (`navigate.ts:532`); after this, a LOCAL target resolves to `root` ≠ current → switch arm → it switches and navigates
  (the intended fix), and only an `smb://` target (which `resolve_location` maps back to the virtual `network` id,
  `commands/volumes.rs:71-89`) still refuses. Switching to the network volume skips `loadDirectory` (DPE:1938) so it
  won't enter the share — acceptable/documented. Update the MCP on-network refusal test to the new trigger.
- **⌘G** (`go-to-path.ts` + `navigate-and-select.ts`): `resolve_go_to_path` is UNCHANGED; `go-to-path.ts` resolves the
  volume on the FE at jump time — `resolveLocation` on the per-variant directory (`resolution.path` for `Directory`,
  `resolution.parentDir` for `File`, `resolution.ancestorDir` for `NearestAncestor`) before the navigate-and-select
  call. Give `navigateToDirInPane`/`navigateToFileInPane` (and `revealFileInBestPane`/`navigateToDirInBestPane`) a
  `Location` param instead of a bare dir string. Unresolved volume → friendly toast.
- **⌘J go-to-latest** (`go-to-latest.ts`): `resolveLocation` the downloaded file's real volume before
  `revealFileInBestPane`.
- **`mirrorLocalStateToPane`** (`DualPaneExplorer.svelte:1597-1609`): migrate its bare `{ path }` (`:1607`) to
  `{ location }` (it knows the source volume id). **Keep its same-path early-return no-op** (`:1603-1605`): the
  `{ location }` arm subsumes the cross-volume→switch and same-volume-different-path→in-place branches, but NOT the
  same-volume **same-path** no-op (routing it through `{ location }` would `navigateToPath(samePath)` → a redundant
  listing reload with cursor/selection churn). Drop only the now-redundant cross-vs-in-place split.
- **FilePane search-results-pane row activation** (replacing `switchVolumeForRealPath`, both branches — `isDirectory` at
  `FilePane.svelte:1864` AND the `redirectToPath` worktree/submodule branch at `:1847`): gate on the EXISTING A6
  classifier `isSearchResultsView` (`FilePane.svelte:306`, `caps.kind === 'search-results'`) — don't re-derive
  `volumeKindOf` and don't add a second detection next to the pre-existing raw compare at `:1817`.
  `resolveLocation(entry.path | entry.redirectToPath)`; route through `navigate()` via a new `onGoToLocation(location)`
  callback → `navigate({ to: { location }, source: 'user' })`. Rationale for a new callback over reusing
  `onVolumeChange` (which would also reach the switch arm): `Location` carries no `volumePath`, and the switch arm
  resolves it via `getVolumePathById`, so a location-only callback is the clean seam — add a one-line note that
  `onGoToLocation` and `onVolumeChange` map to the two intents (go-to-location vs volume-reselect). Leaving the
  `search-results` volume and the snapshot refcount: identical to today — both paths go
  `commitVolumeSwitch → 'push-entry' → pushHistoryEntry`; the `search-results://` entry STAYS in history holding its +1
  (Back still works), and decrements happen only on dropped entries (forward-truncation / cap-eviction / tab-close), so
  there is no "decrement on leaving" to worry about. Unresolved → the SAME friendly toast as `handleSearchNavigate`
  (today this branch silently `log.warn`s — unify it).

### Strings (M2)

- The new failure toasts (search, ⌘G, downloads-reveal, snapshot-row) and the MCP refusal message are user-facing → add
  message-catalog keys in `src/lib/intl/messages/en/{fileExplorer,goToPath,search}.json` with `@key` descriptions (per
  `apps/desktop/CLAUDE.md`; hardcoding fails `cmdr/no-raw-user-facing-string`). Resolve via `tString(...)` like the
  existing nav toasts (`VolumeBreadcrumb.svelte:507`). One shared "couldn't reach that location's drive" string is fine
  across edges. Style: conversational, actionable, no "error"/"failed".

### Tests (M2) — real red→green for the bug logic

- `navigate.test.ts`:
  - **(the bug)** `{ location }` with `volumeId` ≠ current switches volume and loads on the resolved volume (repro: pane
    on an SMB-like fake volume, navigate to a `root` location → switches to `root`).
  - `{ location }` with `volumeId` === current takes the in-place arm (commit-on-listing, not optimistic, `push-path`).
  - `{ volumeId, path }` with `volumeId` === current STILL takes the switch arm (guards the C1 regression: same-volume
    volume-(re)select must not fall into the in-place/refusal path).
  - pinned-tab fork fires on the `{ location }` cross-volume switch.
  - network/MTP refusals still fire, in the right order, for the in-place arm.
- Search: `handleSearchNavigate` resolves the result's volume and navigates with it (mock `resolveLocation`); failure →
  no nav + toast.
- MCP: `nav_to_path` resolves then navigates; unresolvable → `ok: false`.
- Keep green: the former search-results cross-volume tests (subsumed), the has-parent/off-by-one selection tests, the
  persistence round-trip, the navigate refusal-message contract tests.

### Docs (M2)

- `pane/CLAUDE.md` + `pane/DETAILS.md`: rewrite the `navigate()` must-know — two destination shapes (`{ location }` vs
  `{ volumeId, path }`), the arm rules, `isCrossVolumeNavigation`/`snapshot-pane-navigation.ts` gone, snapshot detection
  via capability. Update the L5/R4 narrative in `pane/DETAILS.md:66,140,189`.
- `file-explorer/DETAILS.md:220-224`: update/remove the R4 `isCrossVolumeNavigation` narrative (the canonical home of
  that story). (`file-explorer/CLAUDE.md`'s stale-listing must-know references the cross-volume behavior but not the
  symbol name — adjust wording, don't hunt a symbol that isn't there.)
- Add a short note (nearest the navigation code, e.g. `pane/DETAILS.md`) describing `Location` as navigation's currency
  and the four edge resolvers — the single canonical home for this mechanism.

### Checks (M2)

- Full `pnpm check`. Then feature-specific E2E only (navigation / search-result / go-to-path) via
  `pnpm check desktop-e2e-playwright` scoped to the relevant specs (`test/e2e-playwright/CLAUDE.md`).

---

## Milestone 3 — Verify, polish, finalize

- **Drive the real app via MCP** (CLI fallback `./scripts/mcp-call.sh` if the wired MCP isn't auto-connected —
  `docs/tooling/mcp.md`): confirm the original repro — pane on the SMB/NAS volume → search a local-path result → Enter →
  switches to Macintosh HD, lands on the file, header consistent (tab label and volume selector agree). Smoke ⌘G to a
  path on another volume, MCP `nav_to_path` cross-volume, and ⌘J downloads reveal.
- **Strip milestone tags** from touched code/docs/tests
  (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b' <paths>`), descriptive references instead. The deleted
  `isCrossVolumeNavigation` L5/R4 commentary goes with the code; leave unrelated pre-existing tags alone. Plan file
  keeps its milestone structure.
- **Full `pnpm check --include-slow`**; green.
- Grep for orphaned references to deleted symbols (`isCrossVolumeNavigation`, `snapshot-pane-navigation`,
  `switchVolumeForRealPath`, `resolveVolume` on `NavigateDeps`).
- `CHANGELOG.md`: user-facing bug fix, impact-first ("Search results and Go to path now switch volumes correctly").

---

## Parallelization notes

Sequential by default. Within M1: do 1a first (others import the regenerated `Location`), then 1b ‖ 1c. M2 must land
together — one agent, no concurrent edits (it touches `navigate.ts`, `FilePane.svelte`, `DualPaneExplorer.svelte`, the
type files at once).

## Risk register

- **Same-volume volume-(re)select must keep switch semantics** (the C1 class): network-restore-on-cancel (`DPE:633`),
  cancel walk-up (`:665`), retry (`:710`), `selectVolumeByIndex` re-select (`:1442`), `onVolumeChange` favorite
  (`:1894`), mirror (`:1597`). They stay on `{ volumeId, path }` (always switch). The M2 test asserting
  `{ volumeId, path }` with `volumeId === current` still switches guards this.
- **`NavigateCommit.volumeId` stays optional** — making it required breaks `commitPathFromListing` (`navigate.ts:701`).
- **Don't touch `resolve_go_to_path`** (two consumers incl. a per-keystroke preview); ⌘G resolves its `Location` on the
  FE at jump time via `resolve_location`. Keeps `resolve()` + its tests untouched and avoids a per-keystroke statfs.
- **`Location` specta export** — referenced by the new `resolve_location` command's return type so it reaches
  `bindings.ts`.
- **Persisted-tab compatibility** — composing `Location` keeps `{ volumeId, path }` field names; verify the round-trip
  test. `HistoryEntry` is not persisted.
- **`has-parent.ts`** — comment-only; no `volumeId` string compare (A6).
- **Snapshot detection via capability**, not id string (A6).
- **Snapshot refcount**: no "decrement on leaving" exists — the `search-results://` history entry stays (holding +1);
  decrements fire only on dropped entries. The `{ location }` switch arm uses the same `pushHistoryEntry` path as today,
  so refcount behavior is unchanged.
- **Refusal-message contract** byte-for-byte; the new "unresolvable volume" message is additive.
- **Edge failure UX unified** — search, ⌘G, downloads-reveal, and the FilePane snapshot row all show a friendly toast on
  unresolved volume; MCP returns a typed `ok: false`. No silent aborts.
- **`smb://` → virtual `network` id** (N1): documented limitation. **Indexed result on an unmounted volume** (N2):
  `ResolveLocationResult` can't distinguish "unmounted" from "nonexistent" (both → `location: null`) and the
  no-string-matching rule forbids sniffing — so show the ONE generic friendly toast for all unresolved cases. Don't
  promise a specific "drive not mounted" message unless a typed reason is added to the result (out of scope).
