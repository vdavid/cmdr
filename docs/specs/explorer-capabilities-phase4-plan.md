# Virtual-volume capabilities – Phase 4 execution plan

Just-in-time execution plan for Phase 4 (the FINAL phase) of the
[explorer architecture refactor](explorer-architecture-plan.md). Read that master spec first (§ Target architecture 4
"Virtual-volume capabilities", § Invariants register A6 / A8 / PR1–PR5, § Landmine register L10, § Open questions Q4).
This phase replaces the scattered `volumeId === 'search-results'` / `=== 'network'` capability guards with one typed
capability interface keyed by volume kind, mirroring the spirit of the Rust `Volume` trait. It's the smallest of the
four phases, but it owns removing the known-transitional A6 exceptions that Phases 1–3 deliberately carried, plus the
program-end docs + conformance sweep.

## Loud rules (read before touching anything)

- **A6 is the whole point: guard logic branches on CAPABILITIES, never on volume-id strings.** This is the FE analogue
  of the repo-wide "no string-matching state classification" rule (AGENTS.md). After this phase, "what can a pane do" is
  read from a `VolumeCapabilities` record, not from `=== 'search-results'` / `=== 'network'` scattered across dispatch,
  F-bar, clipboard, transfer, delete, and MCP sync. The litmus for "done": **adding virtual volume #3 costs "implement
  the interface + add one row to the kind table"**, NOT "sweep the codebase for string compares." If the sweep forces
  EVERY string compare through capabilities, it's over-zealous – see § Convert vs keep.
- **NOT a `Record<string, boolean>` bag (master § Target arch 4).** The interface is a real typed
  `interface VolumeCapabilities` PLUS a `VolumeKind` discriminated union. A boolean bag loses the "which kind is this"
  question and invites the same stringly drift in a new shape. The `kind` is the discriminant; the capability fields are
  typed booleans + small unions (`pathScheme`, `listingSource`).
- **L10 / guard strings are CONTRACT.** Every alert title + toast string a capability guard produces survives
  byte-identically; E2E asserts them. The canonical one is `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` ("Search results aren't a
  folder. Paste into a real folder instead."). Enumerate them ALL (§ L10 string register) and pin them in tests before
  moving any guard. A capability refactor that changes a single byte of user-facing copy is a PR3 violation.
- **The seed is `searchResultsVolumeCapabilities()` (master § Target arch 4), GENERALIZED – not deleted-and-rebuilt.**
  Its five flags (`canPasteInto` / `canMkdir` / `canMkfile` / `canRename` / `isSourceOK`) become the `search-results`
  row of the per-kind table. Network and the real kinds (local / smb / mtp) get their own rows derived from what their
  current string-compare guards already encode. Don't invent capabilities no guard reads today (no speculative
  `canSymlink`, etc.). **The rule:** every field retires a real branch in at least one kind, AND a structurally-true
  cell is allowed where the view layer makes the op unreachable for that kind (no guard exists because no guard COULD
  fire) – but only when explicitly documented as such. Example: the `network` row's `canPasteInto` / `canCreateChild` /
  `canRenameInPlace` / `canBeSource: false` cells retire NO branch on their own – a network pane renders
  `NetworkMountView` (`FilePane.svelte:2640`) and never reaches the file-list `{#if}` (`:2763 !isNetworkView`) where
  those ops live, so there's no `=== 'network'` guard to remove. They're structurally-true-no-guard cells: correct,
  unreachable, and marked as such in the table notes. This is the honest reading; don't let the rule and the table
  contradict each other.
- **`volumeKindFor` is the seed CLASSIFIER, not a parallel one to duplicate.** `volume-tint.svelte.ts::volumeKindFor`
  already maps `(volumeId, fsType, category) → 'local' | 'smb' | 'mtp' | 'other'`. It misses the two VIRTUAL kinds
  (`network`, `search-results`) because tinting lumps them into `'other'` (untinted, correct for tint). Phase 4's
  `VolumeKind` is a SUPERSET; resolve the relationship explicitly (§ Q4 / § Kind classifier) – do NOT ship a second
  classifier that disagrees with the first on the same inputs.
- **A8 – no new components.** Capabilities live in a pure `.ts` module + a typed table; the FilePane alt-view descriptor
  (M4) is a pure derivation + the EXISTING `{#if}` chain reading it, not a new wrapper component. Factories, tables, and
  pure helpers only.
- **Namespace/prefix mechanics are NOT capability guards – don't convert them (§ Convert vs keep).** `navigate.ts`'s
  drop-foreign-listings prefix branch (`smb://` / `search-results://`), the synthetic-volume path/name synthesis in
  `DualPaneExplorer` (`leftVolumePath`), the breadcrumb's synthetic `currentVolume` object, MTP path parsing – these
  answer "what string scheme does this namespace use," not "what may the user do here." Forcing them through a boolean
  capability is the "differently complicated" failure mode. They MAY read `pathScheme` off the kind table where that
  removes a literal (a judgment call, justified per site), but they're mechanics, not guards.
- **Per-pane reads only (P1).** Capability lookups happen per-pane (the focused pane for dispatch guards, each pane for
  F-bar / FilePane). No `$derived` reads both panes' kind. The F-bar already obeys this (`getFocusedPane()`); keep it.

## Open question 4 – resolved (FE table keyed by kind; `VolumeInfo` carries no capabilities today)

**Decision: capabilities live in an FE per-KIND table, seeded from `searchResultsVolumeCapabilities()`, keyed by a
`VolumeKind` discriminated union. NOT per-`VolumeInfo` data from Rust.** This matches the master's lean (§ Open
questions Q4) and the evidence:

- **`VolumeInfo` (FE `types.ts:211`, Rust binding) carries NO capability surface today.** It has
  `category: LocationCategory`
  (`'favorite' | 'main_volume' | 'attached_volume' | 'cloud_drive' | 'network' | 'mobile_device'`), `fsType?`,
  `isReadOnly?`, `supportsTrash?`, `smbConnectionState?`. Two of those (`isReadOnly`, `supportsTrash`) ARE per-volume
  capability DATA already consumed by guards (`file-operation-commands.ts:294/413` read-only alerts, delete's
  `supportsTrash`). Those stay per-`VolumeInfo` data – they're genuinely per-volume (a specific USB stick is read-only,
  not the whole "local" kind). The kind table handles the per-KIND structural capabilities (can this namespace host a
  backend listing at all, does it have a `..`, is paste-into meaningful); per-volume runtime flags (`isReadOnly`,
  `supportsTrash`, `smbConnectionState`) layer ON TOP, read from the `VolumeInfo`.
- **The two VIRTUAL kinds (`network`, `search-results`) have NO `VolumeInfo` in the backend `volumes` list** – they're
  synthesized FE-side (`DualPaneExplorer.svelte:456`, `VolumeBreadcrumb.svelte:108`). A per-`VolumeInfo` capabilities
  field couldn't cover them without inventing synthetic VolumeInfos, which is exactly the namespace-mechanics tax we're
  removing. A kind table covers them naturally: `network` and `search-results` are kinds, not data rows.
- **Kinds are derivable from `(volumeId, fsType, category)` today** via `volumeKindFor` (extended for the two virtual
  ids – see § Kind classifier). The classifier is a pure function; the table is a pure
  `Record<VolumeKind, VolumeCapabilities>`. No IPC, no bindings regen.
- **Revisit trigger (master Q4):** if Rust ever grows a `Volume::capabilities()` surface, the FE table becomes a
  fallback/override layer. Until then, FE table. Documented in the new `capabilities.ts` CLAUDE/header.

## The capability interface – seam shape (David reviews this personally)

The concrete shapes built in M1. Marked as the seam-defining commit per master § Verification ("David personally reviews
… the capability interface").

```ts
/** The closed set of volume kinds. The discriminant – every capability lookup goes kind → record. */
export type VolumeKind =
  | 'local' // real filesystem volume (root, attached, cloud_drive, main_volume)
  | 'smb' // mounted SMB share (real backend listing, smb path scheme on the share)
  | 'mtp' // connected MTP storage (real backend listing, mtp:// scheme, no system clipboard)
  | 'network' // the synthetic SMB browser virtual volume (host/share list, smb:// namespace)
  | 'search-results' // the snapshot virtual volume (search-results:// namespace, flat result set)

/**
 * What a pane on a given volume KIND can do. Real typed interface (NOT a
 * `Record<string, boolean>` bag): the `kind` field is the discriminant; the
 * rest are the structural capabilities that current guards branch on. Per-VOLUME
 * runtime flags (isReadOnly, supportsTrash, smbConnectionState) are NOT here —
 * they live on `VolumeInfo` and layer on top (a specific USB stick is read-only,
 * the "local" KIND is not).
 */
export interface VolumeCapabilities {
  kind: VolumeKind
  /** Real backend directory listing exists (drives the alt-view descriptor M4, the git/watcher/space gates). */
  hasBackendListing: boolean
  /** Paste files INTO this pane is meaningful (a real destination folder). False for both virtual kinds. */
  canPasteInto: boolean
  /** Create a child folder/file here (F7 / ⇧F4). False for both virtual kinds. */
  canCreateChild: boolean
  /** Rename the cursor row in-place here (F2). False for the snapshot kind. */
  canRenameInPlace: boolean
  /** This pane can act as the SOURCE of copy/move/delete (snapshot rows are real files ⇒ true). */
  canBeSource: boolean
  /** The system clipboard (⌘C/⌘V) works – needs real local paths. False for mtp + both virtual kinds. */
  supportsSystemClipboard: boolean
  /**
   * Folds ONLY `computeHasParent`'s snapshot rule (`isSearchResultsView ⇒ false`). NOT a complete
   * has-parent answer: the real `hasParent` stays `caps.hasParentRow && currentPath !== '/' && currentPath !== root`,
   * with the two PATH comparisons remaining in `computeHasParent` (a `local` pane at `/`, or any pane sitting on
   * its volume root, has no `..` despite `hasParentRow: true`). False only for search-results.
   */
  hasParentRow: boolean
  /** Mirrors pane state to the MCP PaneState store (network/search panes are skipped – they have other owners). */
  syncsToMcp: boolean
  /** Path namespace the pane's URLs use. Drives the drop-foreign-listings prefix + display, where converting removes a literal. */
  pathScheme: 'filesystem' | 'smb' | 'mtp' | 'search-results'
}

/** Pure: pick the kind for a pane. The single classifier (supersedes the tint one – see § Kind classifier). */
export function volumeKindOf(
  volumeId: string,
  fsType: string | undefined,
  category: LocationCategory | undefined,
): VolumeKind

/** Pure: the capabilities for a kind. Reads the frozen per-kind table. */
export function capabilitiesForKind(kind: VolumeKind): VolumeCapabilities

/** Convenience: classify + look up in one call (the common dispatch/F-bar shape). */
export function capabilitiesFor(volumeId: string, fsType?: string, category?: LocationCategory): VolumeCapabilities
```

The per-kind table (the single source of truth – every cell justified by a current guard it retires):

| kind             | hasBackendListing | canPasteInto | canCreateChild | canRenameInPlace | canBeSource | supportsSystemClipboard | hasParentRow | syncsToMcp | pathScheme       |
| ---------------- | ----------------- | ------------ | -------------- | ---------------- | ----------- | ----------------------- | ------------ | ---------- | ---------------- |
| `local`          | true              | true         | true           | true             | true        | true                    | true         | true       | `filesystem`     |
| `smb`            | true              | true         | true           | true             | true        | true                    | true         | true       | `smb`            |
| `mtp`            | true              | true         | true           | true             | true        | false                   | true         | true       | `mtp`            |
| `network`        | false             | false        | false          | false            | false       | false                   | false        | false      | `smb`            |
| `search-results` | false             | false        | false          | false            | true        | false                   | false        | false      | `search-results` |

Notes binding the table to reality (every `false` retires a branch in at least one kind; structurally-true-no-guard
cells are marked explicitly per the seed rule above):

- **`search-results` row IS `searchResultsVolumeCapabilities()` generalized.** `canPasteInto: false` =
  `canPasteInto: false`; `canCreateChild: false` folds the seed's `canMkdir` + `canMkfile`; `canRenameInPlace: false` =
  `canRename: false`; `canBeSource: true` = `isSourceOK: true`. The two new fields (`hasParentRow`, `syncsToMcp`,
  `hasBackendListing`, `supportsSystemClipboard`, `pathScheme`) come from FilePane's existing per-flag gates, not
  invention.
- **`network`** is the strictest: no listing, no source ops (the host/share list isn't files), no MCP sync
  (`NetworkBrowser` owns that push, `pane-mcp-sync.svelte.ts:149`). `pathScheme: 'smb'` matches the `smb://` drop-prefix
  and the synthetic `smb://` path. **Structurally-true-no-guard cells:** `canPasteInto` / `canCreateChild` /
  `canRenameInPlace` / `canBeSource: false` retire NO branch here – a network pane renders `NetworkMountView`
  (`FilePane.svelte:2640`) and never reaches the file-list `{#if}` (`:2763`), so those ops are unreachable and no
  `=== 'network'` guard exists to remove. The cells are correct (the network pane genuinely can't paste / create /
  rename / source-op) but they're declarations, not branch retirements. The branch-retiring `network` cells are
  `hasBackendListing` (git/watcher/space gates), `syncsToMcp` (`pane-mcp-sync.svelte.ts:149`), and `pathScheme` (the
  drop-prefix).
- **`mtp.supportsSystemClipboard: false`** retires `clipboard-operations.ts`'s `volumeId.startsWith('mtp-')` refusals
  (the "Use F5/F6" toasts) – see § L10. `hasBackendListing: true` because MTP DOES list through the Volume trait (unlike
  the two virtual kinds), which is why MTP panes sync to MCP and get a `..` row.
- **`local` vs `smb` differ only in `pathScheme`** today; they're separate kinds because `volumeKindFor` already
  distinguishes them (tint), and a future SMB-specific capability (no symlink trash, etc.) has a home. Keeping them
  distinct costs nothing and avoids a later table-split.
- **`hasParentRow` is the snapshot rule ONLY, not a standalone has-parent answer.** The column folds
  `computeHasParent`'s FIRST branch (`isSearchResultsView ⇒ false`); the `=== '/'` and `=== root` path comparisons stay
  in `computeHasParent` (see § M3 / the field doc). So the `local: true` cell is NOT "local panes always have `..`" – a
  `local` pane at `/` still has none. And the `network: false` cell is only coincidentally aligned with the real answer
  (a network pane's synthetic `smb://` path equals its synthetic root, so the path comparison would also return false).
  The real per-render answer is `caps.hasParentRow && currentPath !== '/' && currentPath !== root`.
- **The table is `Object.freeze`d** and returned by-reference; `capabilitiesForKind` never allocates (hot-ish: FilePane
  reads it per render). Pin frozenness + purity in M1 tests (the `searchResultsVolumeCapabilities` test already pins
  purity – extend, don't replace).

## Kind classifier – unify with `volumeKindFor`, don't fork (resolves the parallel-classifier risk)

`volume-tint.svelte.ts::volumeKindFor` returns `'local' | 'smb' | 'mtp' | 'other'` – it deliberately collapses
`network` + `search-results` + favorites into `'other'` because tinting wants them UNTINTED. Phase 4's `volumeKindOf`
needs the two virtual kinds as first-class. **Decision: `volumeKindOf` is the superset classifier; `volumeKindFor`
(tint) is re-expressed in terms of it** to keep ONE classification of the same inputs:

- `volumeKindOf` adds the two virtual-id checks FIRST (`volumeId === 'network'` → `'network'`,
  `volumeId === 'search-results'` → `'search-results'`), then falls through to the existing `volumeKindFor` logic for
  the real kinds.
- The tint kind becomes a pure projection: `tintKind(k: VolumeKind): 'local' | 'smb' | 'mtp' | 'other'` mapping
  `network`/`search-results`/(favorite→) to `'other'`. `volume-tint.svelte.ts` calls `volumeKindOf` then `tintKind`, OR
  keeps `volumeKindFor` as a thin `tintKind(volumeKindOf(...))` re-export so its callers and tests
  (`volume-tint.test.ts` – the pure `volumeKindFor` classifier-output pin – plus the tint-render suites
  `volume-tint.svelte.test.ts` / `volume-tint.svelte.fallback.test.ts`) don't churn. Decide by which keeps the tint
  tests byte-stable (preferred: keep `volumeKindFor`'s signature, reimplement its body over `volumeKindOf`).
- **The favorite edge:** `volumeKindFor` returns `'other'` for favorites (untinted). In `volumeKindOf`, a favorite
  resolves to its CONTAINING real volume's kind in practice (a favorite is a virtual id pointing at a real path; panes
  never sit on a bare favorite id at listing time). Confirm this in M1: no live pane carries a favorite `volumeId` when
  a capability is read (the breadcrumb resolves `containingVolumeId`). If one can, `volumeKindOf` maps favorite →
  `local` (its only sane capability set). Pin the chosen behavior in a test.
- **`volumeKindOf` MUST be total – no real input may miss the table (M1 requirement).** `VolumeKind` has NO `'other'`
  member (the two virtual kinds + the three real kinds, nothing else), but `volumeKindFor`'s real-kind logic FALLS
  THROUGH to `'other'` (`volume-tint.svelte.ts:111`) for an unclassified real volume – e.g. an attached drive whose
  `VolumeInfo` arrives with `fsType` AND `category` both `undefined` (it matches none of `mtp` / `smb` /
  `main_volume`/`attached_volume`/`cloud_drive`). If `volumeKindOf` echoed that `'other'`,
  `capabilitiesForKind('other')` would be `undefined` → a crash on the next `.canPasteInto`. **So `volumeKindOf` gets a
  documented DEFAULT branch: real-but-unclassified ⇒ `'local'`** (the only sane capability set for a real, listable
  volume – it has a backend listing, a `..`, system clipboard). This makes the kind → table lookup TOTAL: no
  `volumeKindOf` return value can miss the frozen table. Pin it: an unknown real `volumeId` (`fsType`/`category`
  undefined) resolves to the `local` row, `capabilitiesFor` never returns `undefined`.
  - **Keep the tint projection byte-stable across this default.** The unclassified-real case must still tint as
    `'other'` (untinted) – `volumeKindOf`'s `'local'` default would tint it `tintLocal` if `tintKind` just mapped
    `local → local`. Resolve by NOT routing tint through `volumeKindOf`'s default: either keep `volumeKindFor`'s own
    body as the tint classifier (it still returns `'other'` for the unclassified case, and `volumeKindOf` only DELEGATES
    to it then overrides the fall-through to `'local'`), OR have `tintKind` take the raw `(volumeId, fsType, category)`
    for the tint decision rather than the capability kind. The existing `volume-tint.test.ts` cases that pin `'other'`
    for unclassified ids are the regression anchor – they must stay green.
- **The F-bar and dispatch call sites carry only `volumeId` – the classifier's `fsType`/`category` come from the volume
  store.** `FunctionKeyBar.svelte:58` and `focused-pane-reads.ts:28` expose just the active tab's `volumeId`; `TabState`
  (`tab-types.ts:16`) carries no `fsType`/`category`; only `FilePane.svelte:436` resolves them via
  `getStoreVolumes().find((v) => v.id === volumeId)`. **Decision: `capabilitiesFor(volumeId)` does the volume-store
  lookup internally** – it calls `getVolumes()` (`$lib/stores/volume-store.svelte`), finds the `VolumeInfo`, reads its
  `fsType`/`category`, classifies, and returns the row; callers pass only `volumeId`. Rationale by import topology:
  forcing every F-bar/dispatch call site to replicate the `FilePane:436` find-in-store dance reintroduces the scatter
  the phase removes, and the virtual ids (`network`/`search-results`) have no `VolumeInfo` anyway (the lookup misses,
  and `volumeKindOf` short-circuits on the id before needing `fsType`/`category`). **Topology caveat:** this makes
  `capabilities.ts` depend on `volume-store.svelte`, so it is NOT a pure leaf importing only `types.ts` – update the
  import-cycle note (master § Verification) accordingly: the PURE classifier+table (`volumeKindOf` /
  `capabilitiesForKind`) stays leaf, and `capabilitiesFor` (the store-reading convenience) lives in a thin wrapper that
  may import the store. Keep `capabilitiesForKind(kind)` and `volumeKindOf(volumeId, fsType, category)` callable
  store-free for the FilePane site (which already HAS the `VolumeInfo`) and for tests. M1 ships both: the pure pair +
  the store-reading `capabilitiesFor`.

This is the single seam where the two classifiers touch; getting it wrong reintroduces the drift the phase removes.
David reviews `volumeKindOf` + the table together as the M1 seam.

## Fresh grep (run 2026-06-05, this worktree – HEAD `39d464fe`)

`DualPaneExplorer.svelte` and `FilePane.svelte` are ~2100 / ~2900 lines. Line numbers indicative; re-grep at each
milestone (PR4). The master's § 4 said "navigation, clipboard, transfer, delete, breadcrumb, MCP sync" – Phases 1–3
already collapsed navigation's string compares into `navigate.ts`; this is the post-Phase-3 reality.

### Capability-style guards (CONVERT – this phase's target)

| Site                                                 | Current branch                                                                                                                                     | Capability it becomes                                                                                                                                                         |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `command-dispatch.ts:99`                             | `getFocusedPaneVolumeId() !== 'search-results'` (guard gate)                                                                                       | `!capabilitiesFor(focusedVolId).canPasteInto`-driven                                                                                                                          |
| `command-dispatch.ts:101-107`                        | `blockedBySearchResultsPane` id set (paste/mkdir/mkfile/rename)                                                                                    | per-command `canExecute` reads caps (M2)                                                                                                                                      |
| `FunctionKeyBar.svelte:57-63`                        | `=== 'search-results'` → `canMkdir`/`canMkfile`/`canRename`                                                                                        | `caps.canCreateChild` / `caps.canRenameInPlace` (A6 exception removal)                                                                                                        |
| `clipboard-operations.ts:84,121,148`                 | `volumeId.startsWith('mtp-')` → "Use F5/F6" refusal toasts                                                                                         | `!caps.supportsSystemClipboard` (§ L10)                                                                                                                                       |
| `clipboard-operations.ts:53`                         | `focusedVolId !== 'search-results'` (snapshot clip path gate)                                                                                      | reads `pathScheme === 'search-results'` (mechanics-adjacent; see keep note)                                                                                                   |
| `file-operation-commands.ts:236,288,398`             | `=== 'search-results'` (transfer/delete snapshot-source routing)                                                                                   | `!caps.hasBackendListing` picks the snapshot builder                                                                                                                          |
| `file-operation-commands.ts:294,413`                 | `destVolume?.isReadOnly` (read-only alert)                                                                                                         | **KEEP** – per-`VolumeInfo` data, not kind (Q4)                                                                                                                               |
| `pane-commands.ts:229`                               | `getVolumeId() === 'search-results'` → `isSnapshotPane`                                                                                            | `!caps.hasBackendListing` (or a `isSnapshotPane` cap helper)                                                                                                                  |
| `pane-mcp-sync.svelte.ts:65,149`                     | `deps.getIsNetworkView() \|\| deps.getIsSearchResultsView()` skip (BOOLEAN deps off FilePane deriveds – NOT a `=== 'network'` string compare here) | `!caps.syncsToMcp` (the deps interface gains a `getSyncsToMcp()`/`getCapabilities()` accessor; the A6 win is the SOURCE of the boolean moves to the kind, not the gate shape) |
| `FilePane.svelte:273,283`                            | `isNetworkView`/`isSearchResultsView` deriveds                                                                                                     | derive `caps` once; the alt-view descriptor reads it (M4)                                                                                                                     |
| `FilePane.svelte:345,389,421,648,689,747,825,1230,…` | per-feature `isNetworkView`/`isSearchResultsView`/`isMtpView` gates                                                                                | read `caps` fields where they're capability questions (M4; some stay per-feature)                                                                                             |
| `has-parent.ts:32`                                   | `input.isSearchResultsView` → `false` (one of THREE rules; the `=== '/'` and `=== root` path comparisons stay)                                     | `caps.hasParentRow` folds ONLY the snapshot rule; the two path comparisons remain in `computeHasParent` (L5 – stays coupled to `isCrossVolumeNavigation`)                     |

### Namespace/prefix mechanics (KEEP – not capability guards; § Convert vs keep)

| Site                                              | Why it's mechanics, not a guard                                                                                                                                                                                                                                                                                |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `navigate.ts:531`                                 | `currentVolumeId === 'network'` → on-network REFUSAL string. Stays (it's the L12 refusal source, already centralized in `navigate.ts` by Phase 3; could read `!caps.hasBackendListing` but the refusal MESSAGE is the contract – convert the gate only if it removes the literal without touching the string). |
| `navigate.ts:267` (`validateMtpNavigation`)       | MTP path-scheme parsing + refusal. Path mechanics + L12 string. Keep.                                                                                                                                                                                                                                          |
| `navigate.ts:645,693-694`                         | history-walk network host restore; drop-foreign-listings PREFIX (`smb://`/`search-results://`). Prefix mechanics – MAY read `caps.pathScheme` to derive the prefix (removes the literal), but the branch is namespace, not permission.                                                                         |
| `snapshot-pane-navigation.ts:44-45`               | `isCrossVolumeNavigation` – the snapshot→real-path namespace TRIGGER (L5). Pure namespace mechanics; stays.                                                                                                                                                                                                    |
| `DualPaneExplorer.svelte:456-466`                 | synthetic `smb://` path/name for the `network` virtual volume (not in backend `volumes`). Namespace synthesis. Keep.                                                                                                                                                                                           |
| `DualPaneExplorer.svelte:624,1480,1530,1531`      | network-restore / mirror branches – `navigate({ to: { volumeId: 'network' } })` plumbing. Volume-id IDENTITY, not capability. Keep.                                                                                                                                                                            |
| `VolumeBreadcrumb.svelte:108,110,489,212`         | synthetic `currentVolume` object + display label + network-disabled gate. DISPLAY + identity. Keep (see display note).                                                                                                                                                                                         |
| `volume-grouping.ts:35,41,81` / `volume-tint:102` | `category === 'network'`/`'smbfs'` – kind CLASSIFICATION inputs (feed `volumeKindOf`). Keep as classifier internals.                                                                                                                                                                                           |
| `app-status-store.ts:103,105,375,389`             | `=== 'network'` → skip path-resolution on persist (virtual paths aren't filesystem). Persistence mechanics. Keep.                                                                                                                                                                                              |
| `initialization.ts:46`                            | `=== 'network'` → trust stored id (virtual volume, no `resolvePathVolume`). Startup mechanics. Keep.                                                                                                                                                                                                           |
| `mtp-path-utils.ts`, `navigate.test.ts`, `*.test` | path parsing + test fixtures. Keep.                                                                                                                                                                                                                                                                            |
| `TransferDialog.svelte:161`, `DebugHistoryPanel`  | `category !== 'network'` volume-list filter (display); debug panel. Keep.                                                                                                                                                                                                                                      |

### Display logic (KEEP – breadcrumb labels, view selection)

`VolumeBreadcrumb.svelte` (search-results "Search results" label, network label) and
`FilePane.svelte::breadcrumbDisplayPath:389` (snapshot label as path) are DISPLAY. They answer "what do I render," not
"what may the user do." Keep – but the FilePane alt-view `{#if}` CHAIN (M4) is the one display site that DOES become a
capability-driven descriptor (it's structural view selection, not label text).

### Deviations from master spec

1. **Navigation's string compares already collapsed into `navigate.ts` (Phase 3).** Master § 4 listed "navigation" as a
   sweep target; Phase 3 already centralized the `network`/`search-results` navigation branches into ONE module
   (`navigate.ts`). Phase 4 does NOT re-sweep them – they're namespace mechanics + the L12 refusal source, already
   single-sited. The convert targets are dispatch, F-bar, clipboard, transfer/delete, pane-commands, MCP-sync, FilePane
   alt-view. This SHRINKS milestone 3 vs the master's framing.
2. **No `canExecute` exists in the registry today (verified `command-registry.ts` / `types.ts`).** The master (§ Target
   arch 2) implied Phase 2 might leave a `canExecute` hook; it did not – `Command` has
   `{ id, name, scope, showInPalette, shortcuts, description?, keywords? }`, no enablement. M2 ADDS the consolidation as
   a dispatch-side capability gate (one `blockedByCapabilities` reading the table), NOT a new registry field – the F-bar
   `disabled` flags + the dispatch guard + the MCP errors all read the SAME `capabilitiesFor` call. This is "declared
   once" via a shared reader, not a registry schema change (which would touch ~115 commands for 5 guards).
3. **`isReadOnly` / `supportsTrash` are per-VOLUME, not per-kind (Q4 nuance).** The master's interface sketch listed
   `canWrite` / `supportsTrash` as capability fields. Those are runtime per-`VolumeInfo` data (a specific read-only USB
   stick), already correctly read from `VolumeInfo` at `file-operation-commands.ts:294/413`. They do NOT move into the
   kind table – that would wrongly make ALL `local` volumes read-only-or-not as one. The kind table is STRUCTURAL
   capability; per-volume flags layer on top. Documented in § Q4.
4. **`mtp` is a convert target the master under-emphasized.** The clipboard MTP refusals (`startsWith('mtp-')`) ARE
   capability guards (`supportsSystemClipboard: false`) and convert in M3, alongside the search-results sweep – the
   master's § 4 named only `network`/`search-results`. Including MTP makes the table honest (3 real kinds + 2 virtual)
   and is what lets "virtual volume #3" truly cost one row.
5. **`volumeKindFor` already exists (tint).** The master said "the existing `searchResultsVolumeCapabilities()` is the
   seed." There's a SECOND seed – the kind classifier. M1 unifies them (§ Kind classifier) rather than shipping a third.

## Milestones

Each milestone is atomic (add + migrate + delete old branch; PR1). Gates per milestone: `--fast` continuously during
work; full `pnpm check` + `--check desktop-e2e-linux` before the milestone commit. Phase-end (after M4 + the program-end
sweep): `--include-slow` (adds macOS Playwright + `rust-tests-linux`), then watch CI to green before merging to `main`.
PR3 (byte-identical behavior, esp. L10 strings) gets EXTRA scrutiny. Import-cycle rule (master § Verification): the PURE
core – `VolumeKind`, `VolumeCapabilities`, the frozen table, `volumeKindOf`, `capabilitiesForKind` – is a leaf importing
ONLY `types.ts` (`LocationCategory`) + `mtp-path-utils.ts` (`isMtpVolumeId`, already a leaf). The store-reading
convenience `capabilitiesFor(volumeId)` ALSO imports `volume-store.svelte` (`getVolumes`) to resolve `fsType`/`category`
from the `VolumeInfo` – so the module is NOT a strict `types.ts`-only leaf (§ Kind classifier). That's fine for cycles
as long as `volume-store.svelte` doesn't import back from capabilities (verify with `import-cycles` in M1); the pure
pair stays store-free for the FilePane site and tests. The module is imported by dispatch, F-bar, clipboard, file-ops,
pane-commands, mcp-sync, FilePane, has-parent; it imports NOTHING from `routes/` or from any consumer. PR5: the phase
reverts as one merge range.

### M1 – Capability interface + per-kind table + classifier unify + tests (the seam)

**Scope:** new `lib/file-explorer/capabilities/volume-capabilities.ts` (or colocate in `pane/`) exporting `VolumeKind`,
`VolumeCapabilities`, the frozen per-kind table, `volumeKindOf`, `capabilitiesForKind`, `capabilitiesFor`. Re-express
`volumeKindFor` (tint) over `volumeKindOf` so there's ONE classifier (§ Kind classifier). NO consumer migration yet —
the string-compare guards still run; M2–M4 swap them. This is the seam-defining commit – **flag for David's review**
(the interface, the `kind` union, the table, the classifier unify, the Q4 per-kind-vs-per-volume split).

**Intentions:**

- Generalize `searchResultsVolumeCapabilities()`: its five flags become the `search-results` table row, byte-equivalent
  in meaning. Keep `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` exported from `lib/search/capabilities.ts` (it's the L10 string;
  consumers import it from there). Decide: does `lib/search/capabilities.ts` re-export from the new module, or does the
  new module own the table and `lib/search/capabilities.ts` shrink to just the toast string? (Lean: new module owns the
  table + classifier; `lib/search/capabilities.ts` keeps the toast string + a thin
  `searchResultsVolumeCapabilities = () => capabilitiesForKind('search-results')` shim until M4 retires its sole caller
  (`SearchResultsView.svelte:84`, an M4-scope file – see § Shim-retirement sequencing below), then delete the shim.
  Don't orphan the toast string – it's imported by `command-dispatch.ts:26` and `capabilities.test.ts`.)
  - **The shim returns the NEW field names, so M1 MUST migrate its one live consumer in the same commit or it won't
    typecheck.** `SearchResultsView.svelte:84` calls `searchResultsVolumeCapabilities()` and line 216 reads
    `caps.canRename` – the OLD field name. `capabilitiesForKind('search-results')` returns `canRenameInPlace`, not
    `canRename`, so a strict-TS build breaks the moment the shim's return type changes. **Decision: M1 migrates
    `SearchResultsView.svelte:216` to `caps.canRenameInPlace`** (the lean is "new module owns the table," so the shim
    yields the new shape; renaming the one read site is cheaper than preserving the old field names on the shim's return
    type). `SearchResultsView.svelte` is an M4-scope file, but this single field rename rides in M1 with the shim it
    depends on – the rest of the file's `caps`/descriptor work stays in M4.
- The table is `Object.freeze`d; `capabilitiesForKind` returns the frozen reference (no allocation).
- `volumeKindOf` adds the two virtual-id checks before delegating to the real-kind logic; the favorite edge resolves per
  § Kind classifier (pin the chosen behavior).

**Shim-retirement sequencing.** `searchResultsVolumeCapabilities()` has exactly ONE live caller today:
`SearchResultsView.svelte:84` (re-verified this worktree). That file is M4-scope (the alt-view / descriptor work). So
the shim survives through M2 and M3 and retires in **M4**, when `SearchResultsView` moves onto the FilePane `caps`
descriptor – NOT "M2/M3 retire its callers" (there are no dispatch/F-bar/clipboard callers of the shim; those sites read
the kind table directly via `capabilitiesFor`, never the search-results shim). M1 keeps the shim AND migrates that one
caller's field read (`:216` `caps.canRename` → `caps.canRenameInPlace`) so the shim's new-shape return typechecks from
the start; the shim itself is deleted in M4.

**Test plan (TDD, new file → covered by the 70%
`src/lib/**`gate):** every kind → its exact row;`volumeKindOf`for each (real volume by category/fsType, MTP id, the two virtual ids, the favorite edge); frozenness + purity (extend the`capabilities.test.ts` purity pin); the tint projection (`volumeKindFor`/`tintKind`still returns`'other'`for the two virtual kinds, byte-stable against the existing tint tests). The pure`volumeKindFor`assertions live in`volume-tint.test.ts`(the classifier-output pin – 12 cases);`volume-tint.svelte.test.ts`+`volume-tint.svelte.fallback.test.ts`cover the tint-render paths. All three stay green (the classifier unify must not change tint output). **Totality (fix the table-miss crash):** an unknown/unclassified real`volumeId` (`fsType`and`category`both`undefined`) resolves to the `local`row, and`capabilitiesFor`/`capabilitiesForKind(volumeKindOf(...))`NEVER returns`undefined` for any input – pin this explicitly (`volumeKindOf`has no`'other'`escape; the default branch maps real-but-unclassified →`local`). **Store-lookup path:** `capabilitiesFor(volumeId)`with a`volumeId`absent from the volume store (the two virtual ids, or a stale id) still classifies correctly (virtual ids short-circuit before the store lookup; a missing real id falls to the`local`default). Mock`getVolumes()`
in the test.

**DONE:** interface + union + table + classifier land with tests; tint tests + `capabilities.test.ts` stay green; NO
capability-GUARD consumer migrated yet (the dispatch / F-bar / clipboard / file-ops string compares still run) – the
ONLY code change beyond the new module is the single field-name rename at `SearchResultsView.svelte:216`
(`caps.canRename` → `caps.canRenameInPlace`) required so the shim's new-shape return typechecks; dead-code-free because
the table IS consumed by its own tests + the shim – if `knip` flags the table's exports as unused, land M1+M2 together,
the precedent the master allows; `--fast` + full suite + `desktop-e2e-linux` green; **David reviews the seam.**

### M2 – Command `canExecute` consolidation (F-bar + dispatch + MCP read ONE source)

**Scope:** the three capability-gate sites for the SAME action set (paste / mkdir / mkfile / rename) read ONE
`capabilitiesFor(focusedVolId)` call instead of three independent `=== 'search-results'` derivations. Concretely:

- **Dispatch (`command-dispatch.ts`):** rewrite `blockedBySearchResultsPane` →
  `blockedByCapabilities(commandId, explorer)`: classify the focused pane's kind, look up caps, map the blocked command
  set to capability fields (`edit.paste`/`edit.pasteAsMove` → `!canPasteInto`; `file.newFolder`/`file.newFile` →
  `!canCreateChild`; `file.rename` → `!canRenameInPlace`), toast `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` on block. **L10:**
  the toast string is unchanged; only the GATE moves from string-compare to capability. The id set is unchanged (same
  five commands). Keep the function name aligned with what it does now (AGENTS.md "name internals after the UI" – it
  guards by capability, not by "search-results pane"; rename, update `routes/(main)/CLAUDE.md` § "Search-results pane
  guard").
- **F-bar (`FunctionKeyBar.svelte`):** replace the A6-exception `isSearchResultsPane` derived (`:57`) +
  `canMkdir`/`canMkfile`/`canRename` (`:61-63`) with one
  `caps = $derived(capabilitiesFor(getActiveTab(...).volumeId, ...fsType, ...category))` then
  `canMkdir = caps.canCreateChild` etc. **This removes the known-transitional A6 exception** the F-bar's header comment
  (`:53-56`) and `pane/CLAUDE.md:150-155` flag as Phase-4-owned. The store-read reactivity (A9) is unchanged; only the
  comparison changes. Update both the component comment and the CLAUDE.md note (drop "known-transitional A6 exception").
  `canSourceOps` stays a prop (`+page.svelte:969` hardcodes `true`) – it maps to `caps.canBeSource`, but moving it off
  the prop is a separate concern; the master kept it a prop. Decide: wire it to `caps.canBeSource` now (it's already a
  capability) or leave the `={true}` prop. Lean: wire it – it's the same one-source consolidation, and the prop is
  dead-true today.
- **MCP errors:** the master's "MCP errors read one source" – the MCP path for these commands routes through
  `mcp-listeners.ts` → `dispatch` → `handleCommandExecute` → `blockedByCapabilities` (Phase 2 wired MCP through the bus,
  `routes/(main)/CLAUDE.md`). So MCP ALREADY shares the dispatch guard after Phase 2; M2's job is to confirm the single
  `blockedByCapabilities` is the one source all three (keyboard, F-click, MCP) hit, and that no separate MCP-side
  string-compare survives. Grep `mcp-listeners.ts` + `pane-commands.ts` for residual `=== 'search-results'` capability
  gates → the `pane-commands.ts:229` `isSnapshotPane` is a DIALOG-data flag (not a guard), convert it in M3 with the
  other source-routing sites, not here.

**Landmines:** L10 (the toast + any alert strings byte-identical – pin in test BEFORE the rewrite). A6 (the F-bar gate
now reads caps, not a string). The F-bar `disabled` flag still wins first (a disabled button can't fire the dispatch
guard) – that ordering is unchanged; the guard set still matches the disabled set exactly (paste isn't an F-button, so
the dispatch guard remains the only path for `edit.paste`).

**Test plan:** the existing F-bar disablement tests + `command-dispatch` search-results-guard tests stay green (re-point
their expectations at the capability gate, same outcomes). Add a focused test: `blockedByCapabilities` returns
true+toast for each blocked id on a `search-results` pane, false on a `local` pane, AND false on a `network` pane for
the non-paste ids while paste is blocked (network can't paste either – a NEW row the string-compare era didn't cover,
but harmless: network panes don't surface F7/F2). Pin `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` byte-for-byte.

**DONE:** F-bar + dispatch + (via-bus) MCP read one `capabilitiesFor`; the A6-exception comment + CLAUDE note removed;
L10 strings byte-identical; `--fast` + full suite + `desktop-e2e-linux` green.

### M3 – Sweep the source-routing + clipboard + MCP-sync + has-parent string compares (A6)

**Scope:** convert the remaining capability-style `=== 'search-results'` / `startsWith('mtp-')` guards to capability
reads, deleting the string compares. Per site:

- **Clipboard (`clipboard-operations.ts`):** `startsWith('mtp-')` MTP refusals (`:84,121,148`) →
  `!caps.supportsSystemClipboard` + the EXACT existing toasts ("Use F5 to copy files from MTP devices", "Use F6 to move
  files from MTP devices", "Use F5 to copy files to MTP devices" – § L10, three DISTINCT strings). The snapshot-clip
  gate (`:53` `!== 'search-results'`) reads `caps.pathScheme === 'search-results'` (it's selecting the snapshot path
  resolver – namespace-adjacent, but the gate is "does this pane use the snapshot scheme," a capability-table read that
  removes the literal). Keep the `currentPath.startsWith('search-results://')` PREFIX parse (`:57`) – that's extracting
  the snapshot id from the URL, pure namespace mechanics.
  - **PR3 caution – the MTP refusal set WIDENS under conversion (verify it's unreachable).** Today's clipboard guards
    match `volumeId.startsWith('mtp-')` ONLY, but `caps.supportsSystemClipboard: false` is reached via `volumeKindOf`'s
    MTP arm = `volumeKindFor`'s `isMtpVolumeId(volumeId) || category === 'mobile_device'`, where `isMtpVolumeId`
    (`mtp-path-utils.ts:72`) = `volumeId.includes(':') || volumeId.startsWith('mtp-')`. So the capability gate ALSO
    refuses colon-form ids and `mobile_device`-category panes that the `startsWith('mtp-')` gate let through.
    **Decision: pin that the widening is unreachable, not intentional hardening** – live MTP panes carry
    `mtp-{deviceId}-{storageId}` volumeIds (which already match `startsWith('mtp-')`), so no real clipboard-time pane is
    colon-form-only or `mobile_device`-only. M3 adds a test asserting that for every volumeId a pane can hold when a
    clipboard op fires, `volumeId.startsWith('mtp-') === !capabilitiesFor(volumeId).supportsSystemClipboard` (the
    converted gate is byte-equivalent on the live input set). If that test can't be made to hold (some real pane IS
    colon-form/`mobile_device`-only), STOP and re-decide: either narrow the MTP arm for the clipboard read or accept +
    document the hardening explicitly. The default expectation is byte-identical (PR3), proven by the equivalence test.
- **Transfer/delete (`file-operation-commands.ts`):** `=== 'search-results'` source-routing (`:236`
  transfer-from-snapshot, `:398` delete-from-snapshot) → `!caps.hasBackendListing` picks the snapshot builder (no
  backend listing ⇒ resolve from the snapshot store). The DEST guard `:288` (`destVolId === 'search-results'` →
  `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) → `!caps.canPasteInto` + the unchanged toast (§ L10). The `isReadOnly` alerts
  (`:294,413`) STAY (per-`VolumeInfo` data, Q4). The `SEARCH_RESULTS_PREFIX` parse inside `openDeleteFromSearchResults`
  (`:331`) STAYS (namespace id extraction).
- **`pane-commands.ts:229`:** `isSnapshotPane: getVolumeId() === 'search-results'` → `!caps.hasBackendListing` (or a
  named `isSnapshotPane(volumeId)` helper off the table). It flags the Selection dialog's banner – a capability question
  ("is this a snapshot view"), convert it.
- **MCP-sync (`pane-mcp-sync.svelte.ts:65,149`):** `deps.getIsNetworkView() || deps.getIsSearchResultsView()` →
  `!caps.syncsToMcp`. **Note the gate is already BOOLEAN here, not a string compare** – `pane-mcp-sync` reads
  `getIsNetworkView()` / `getIsSearchResultsView()` accessors off its `deps` (FilePane deriveds), so this site has no
  `=== 'network'` literal to delete. The conversion moves the SOURCE of those booleans onto the kind: the deps interface
  gains a `getSyncsToMcp()` (or `getCapabilities()`) accessor; FilePane supplies it from its derived caps, and the two
  `getIs*View()` deps retire. Keep the rich comment explaining WHY network/search skip (NetworkBrowser owns the push) –
  it's the constraint the capability encodes, per `docs-maintenance.md` (keep the named-incident rationale).
- **`has-parent.ts:32`:** `input.isSearchResultsView` → `caps.hasParentRow` (`false` for search-results).
  **`hasParentRow` is NOT a full has-parent answer** – `computeHasParent` returns false for THREE cases
  (`has-parent.ts:32`), only the first of which is per-kind: `isSearchResultsView` (the snapshot rule `hasParentRow`
  folds), `currentPath === '/'`, and `currentPath === effectiveVolumeRoot`. A `local` pane at `/` has no `..` even
  though `hasParentRow: true`. So the swap is: `hasParentRow` replaces ONLY the `isSearchResultsView` branch; the two
  PATH comparisons (`=== '/'`, `=== root`) STAY in `computeHasParent`. The real answer is
  `caps.hasParentRow && currentPath !== '/' && currentPath !== root`. **L5 caution:** `computeHasParent` MUST stay
  coupled to `isCrossVolumeNavigation` (master L5, `pane/CLAUDE.md:91`); `hasParentRow` encodes the SAME snapshot rule
  (snapshot ⇒ no `..`); don't decouple the two. The input shape can keep `isSearchResultsView` as a derived-from-caps
  boolean OR take the `hasParentRow` field directly – pick the one that keeps `has-parent.test.ts` byte-stable (lean:
  keep the `HasParentInput` struct, fill its first field from `caps.hasParentRow`, leave the two path comparisons
  untouched). Re-pin `has-parent.test.ts` (snapshot → all indices, real → skip index 0, AND the existing `/`-root and
  volume-root cases stay green).

**Landmines:** L10 – enumerate, pin, verify byte-identical. The strings touched or adjacent in M3:

- The three MTP clipboard toasts (`clipboard-operations.ts:84,121,148`) – these CONVERT (string-compare →
  `!caps.supportsSystemClipboard`), copy unchanged.
- The dest-paste toast (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`, `file-operation-commands.ts:288`) – CONVERTS, copy
  unchanged.
- FIVE distinct read-only alert strings that STAY (per-`VolumeInfo` `isReadOnly`, not kind – Q4):
  `file-operation-commands.ts:42` ('Read-only volume' / "…Renaming isn't possible here."), `:75` ('Read-only volume' /
  "…Creating folders isn't possible here."), `:104` ('Read-only volume' / "…Creating files isn't possible here."),
  `:294` ('Read-only device', the dest alert), `:413` ('Read-only volume' / "…Deleting files isn't possible here."). All
  five share the era's title pattern but carry DISTINCT bodies; the L10 byte-pin must list all five even though none of
  them move.

L5 (has-parent ⟷ cross-volume coupling intact). A6 (every converted site reads caps, zero new string compares
introduced). **MTP clipboard-guard breadth (PR3):** the converted `!caps.supportsSystemClipboard` gate is BROADER than
today's `startsWith('mtp-')` (it also catches colon-form ids + `mobile_device`-category panes via
`isMtpVolumeId`/`volumeKindFor`); pin that this widening is unreachable on the live clipboard-time pane set (see the
Clipboard bullet's equivalence test) so PR3's byte-identical claim holds. The `isPathOnVolume` / prefix mechanics in
`navigate.ts` are NOT touched (namespace, kept M-scope-out).

**Test plan:** clipboard tests (MTP refusal toasts byte-identical, snapshot copy/cut paths unchanged); the MTP
breadth-equivalence pin (`startsWith('mtp-') === !capabilitiesFor(volumeId).supportsSystemClipboard` across the live
clipboard-time pane volumeId set – proves the widening is unreachable); transfer/delete snapshot-source + dest-block
tests; `has-parent.test.ts` re-pinned; `pane-mcp-sync` skip behavior (the three SMB share-count E2E that the network
skip protects stay green – they're the regression anchor for `syncsToMcp: false`). Re-grep `=== 'search-results'` /
`=== 'network'` / `startsWith('mtp-')` across the CONVERT set → ZERO capability-guard hits remain (only the KEEP-set
mechanics + display + tests). The A6 sweep grep is the gate.

**DONE:** clipboard / transfer / delete / pane-commands / mcp-sync / has-parent read caps; the L10 strings
byte-identical; A6 grep shows only mechanics/display/test residue; `--fast` + full suite + `desktop-e2e-linux` green.

### M4 – FilePane alt-view chain through a content descriptor

**Scope:** the `{#if}` alt-view chain (`FilePane.svelte:2617-2723+`: `unreachable` → SMB-reconnecting → SMB-gave-up →
`isNetworkView` → `isSearchResultsView` → `isMtpDeviceOnly` → `smbUpgradeLogin` → `loading` → `friendlyError` → `error`
→ list) resolves the VIRTUAL-VOLUME view selection (network / search-results) through the kind, and the per-feature
gates (git/watcher/space/MCP/dir-poll) read `caps` where they're capability questions. **Be honest about what's
expressible vs genuinely per-feature (master § 4 – "investigate what's actually expressible"):**

- **A `viewDescriptor` derived** picks the alt-view for the KIND-driven branches: `network` → `NetworkMountView`,
  `search-results` → `SearchResultsView`, `mtp-device-only` → `MtpConnectionView`. These three ARE pure functions of
  kind (+ the device-only sub-state for MTP). Express them as a small `paneViewKind` derivation
  (`'network' | 'search-results' | 'mtp-connect' | 'normal'`) off `caps.kind` (+ `isMtpDeviceOnly`), and let the `{#if}`
  chain branch on it. **This is NOT a new component (A8)** – it's a derived discriminant the existing chain reads.
- **The RUNTIME-state branches STAY per-feature, NOT kind-driven:** `unreachable` (volume resolution timed out —
  runtime), `showSmbReconnecting` / `showSmbGaveUp` (reconnect-manager state – runtime), `smbUpgradeLogin` (inline login
  flow – runtime), `loading` / `friendlyError` / `error` (listing lifecycle – runtime). These are PER-EXECUTION states,
  not "what may this kind do." Forcing them through capabilities is the over-zealous failure mode. They keep their own
  reactive flags. The descriptor governs ONLY the kind-structural view choice; runtime states gate IN FRONT of it
  exactly as today (the ordering is byte-identical – `unreachable` still wins over `isNetworkView`, etc.).
- **The per-feature gates** (`syncGitState:345` `isNetworkView || isSearchResultsView`, the listing watcher `:689`, the
  dir-exists poll `:2500`, MCP sync `:149`, the git/space gates): each reads `!caps.hasBackendListing` where the
  question is "is there a real directory to watch/poll/git/sync." `isMtpVolumeId(volumeId)` checks in those gates STAY
  as MTP path/feature checks where MTP-specific (e.g. git can't run on MTP – that's `caps.hasBackendListing` is true for
  MTP but git still skips; so the git gate is `hasBackendListing && !isMtp` – DON'T collapse MTP's no-git into a generic
  capability unless a `supportsGit` field earns its place; lean: leave the `isMtpVolumeId` check, convert only the
  `isNetworkView || isSearchResultsView` half to `!caps.hasBackendListing`). Justify each gate per-site: convert the
  ones that are "no backend listing"; leave the ones that are MTP-path-specific.
- **`isNetworkView` / `isSearchResultsView` deriveds (`:273,283`)** become `caps.kind === 'network'` /
  `caps.kind === 'search-results'` OR are kept as named deriveds off `caps` (readability – a named
  `isNetworkView = $derived(caps.kind === 'network')` is fine and keeps call sites terse; the A6 win is that the SOURCE
  of truth is the kind, not a raw `volumeId ===` per site). Lean: keep the named deriveds, re-source them from
  `caps.kind` once.

**Landmines:** L10 (no alt-view here produces a guard toast, but the view-selection ORDER is user-visible – keep the
`{#if}` precedence byte-identical). A8 (no new component). The MTP device-only sub-state (`isMtpDeviceOnly:431`) is a
SUB-kind distinction the table doesn't carry (it's `mtp` kind + "not yet connected") – keep it as a FilePane-local
derived feeding the descriptor; don't force it into the kind union (it's a connection state, not a kind).

**Test plan:** FilePane alt-view rendering – each kind renders its view (network → NetworkMountView, search-results →
SearchResultsView, mtp-device-only → MtpConnectionView, normal → list); the runtime-state precedence
(unreachable/reconnecting/gave-up/login/loading/error still win in order) unchanged. The per-component a11y sweeps
(`*.a11y.test.ts`) stay green. Re-grep `volumeId === 'network'` / `=== 'search-results'` inside `FilePane.svelte` → only
the `caps`-sourced named deriveds + any justified MTP-path mechanics remain.

**DONE:** the alt-view kind-selection reads a `caps.kind`-driven descriptor; runtime-state branches stay per-feature
(justified); the converted per-feature gates read `!caps.hasBackendListing`; the kept MTP-path gates documented; A8
honored; `--fast` + full suite + `desktop-e2e-linux` green.

## Program-end checklist (this is the LAST phase)

After M4, before the phase merge – the whole-refactor close-out:

- **A6 final grep across `apps/desktop/src/`** for `=== 'search-results'` / `=== 'network'` / `startsWith('mtp-')` (and
  the `!==` forms): every remaining hit is on the KEEP list (navigate.ts namespace/refusal, synthetic-volume synthesis,
  breadcrumb display, classifier internals, persistence/init mechanics, tests, debug panel). NO capability guard
  survives. Document the residue inventory in `capabilities/CLAUDE.md` so a future agent doesn't "finish the sweep" on a
  mechanics site.
- **Remove ALL remaining known-transitional markers:** the F-bar A6-exception comment (`FunctionKeyBar.svelte:53-56`,
  done in M2), the `pane/CLAUDE.md` "known-transitional A6 exception" note (`:150-155`), any "Phase 4 owns this" /
  "known-transitional" string left in `pane/CLAUDE.md` / `search/CLAUDE.md` / `file-explorer/CLAUDE.md` from Phases 1–3.
  Grep `transitional` / `Phase 4 owns` across `apps/desktop/src/**/CLAUDE.md` → zero.
- **Docs final sweep (master § Docs updates):**
  - `docs/architecture.md` frontend section: reflect ALL FOUR phases – the explorer store (`explorer-state.svelte.ts`),
    the typed command bus, the `navigate()` transaction + persistence subscriber, and NOW the capability interface. The
    frontend explorer area is the module store + dispatch + transaction + capabilities, no longer the imperative-API
    braid the master § Goal described.
  - New `capabilities/CLAUDE.md` (or section in `pane/CLAUDE.md`): the `VolumeKind` union, the table, the classifier
    unify with tint, the Q4 per-kind-vs-per-volume split, the convert-vs-keep boundary (so future agents know namespace
    mechanics ≠ capability guards), and "to add virtual volume #3: add a kind + a table row + a `volumeKindOf` branch."
  - `pane/CLAUDE.md` / `search/CLAUDE.md` / `file-explorer/CLAUDE.md`: re-point every `volumeId === 'search-results'`
    capability narration at the capability read; keep the namespace/display narrations.
  - `routes/(main)/CLAUDE.md` § "Search-results pane guard": rename to the capability guard, update the function name.
- **Final conformance pass against the master's FULL invariants register** (P1–P5, A1–A9, PR1–PR5): a short reviewer
  pass confirming Phase 4 honored A6 (capabilities, not strings) + A8 (no new components) + P1 (per-pane reads) and
  didn't regress any earlier-phase invariant (the store's A1/A2, the bus's A3, the transaction's A4/A5). This is the
  refactor's exit gate – the master's "David personally reviews the seam-defining commit of each phase" applies to the
  capability interface (M1), and the program-end pass is the cumulative check.
- **Phase-end gates:** `--include-slow` green (macOS Playwright + `rust-tests-linux`) + watch CI to green before merging
  to `main`.

## Invariants this phase must honor

- **A6** (loud) – every capability guard reads `VolumeCapabilities`, never a volume-id string. The M3 + program-end
  greps are the gate. Namespace/prefix mechanics + display + classifier internals + persistence are the justified
  residue, inventoried in `capabilities/CLAUDE.md`.
- **A8** – no new components. `capabilities.ts` is a pure module + table; the FilePane descriptor (M4) is a derived
  discriminant read by the existing `{#if}` chain.
- **L10** – every guard alert/toast string byte-identical: the snapshot paste toast
  (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`), the three MTP clipboard toasts, the dest-paste toast, and the FIVE distinct
  read-only alert strings (`file-operation-commands.ts:42/75/104/294/413` – three 'Read-only volume' bodies for
  rename/mkdir/mkfile, the 'Read-only device' dest alert, and the delete 'Read-only volume' body). Pinned per milestone
  BEFORE the rewrite.
- **L5** – `computeHasParent` (now `caps.hasParentRow`) stays coupled to `isCrossVolumeNavigation`; the snapshot no-`..`
  rule is identical.
- **P1** – capability lookups are per-pane (focused for dispatch, per-pane for F-bar/FilePane); no `$derived` reads both
  panes' kind.
- **A9** – the F-bar's store-read reactivity is unchanged; only the comparison moves from string to capability.
- **PR1/PR3** – each milestone add+migrate+delete atomic; byte-identical user-visible behavior (toast copy, F-bar
  disablement, alt-view precedence, MCP-skip behavior). EXTRA PR3 scrutiny on L10.
- **PR4** – the first task of each milestone is the fresh re-grep, never this plan's tables (they drift). Any feature
  landing on `main` mid-phase that adds a volume-id capability branch gets caught by the re-grep and added to the
  milestone checklist.
- **PR5** – the phase reverts as one merge range; M1–M4 are cumulative, not independently revertable.
