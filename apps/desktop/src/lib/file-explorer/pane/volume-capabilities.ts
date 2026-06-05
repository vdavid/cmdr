/**
 * Volume capabilities — the single FE source of truth for "what can a pane on a
 * given volume KIND do".
 *
 * This is the Phase-4 seam of the explorer refactor: it replaces the scattered
 * `volumeId === 'search-results'` / `=== 'network'` / `startsWith('mtp-')`
 * capability guards (dispatch, F-bar, clipboard, transfer/delete, MCP sync,
 * has-parent, FilePane alt-view) with one typed `VolumeCapabilities` record
 * keyed by a closed `VolumeKind` discriminated union. Adding virtual volume #3
 * costs "add a kind + a table row + a `volumeKindOf` branch", NOT "sweep the
 * codebase for string compares" (invariant A6).
 *
 * Two layers:
 *  - The PURE core — `VolumeKind`, `VolumeCapabilities`, the frozen per-kind
 *    table, `volumeKindOf`, `capabilitiesForKind` — is a leaf: it imports only
 *    `volume-tint.svelte` (for the shared real-kind classifier `volumeKindFor`)
 *    and `types.ts` (`LocationCategory`). No `routes/`, no consumers.
 *  - The store-reading convenience `capabilitiesFor(volumeId)` resolves the
 *    `fsType`/`category` from the volume store so callers that hold only a
 *    `volumeId` (F-bar, dispatch) don't have to replicate the find-in-store
 *    dance. It additionally imports `volume-store.svelte`.
 *
 * ## Per-KIND vs per-VOLUME
 *
 * The table carries STRUCTURAL, per-kind capabilities (can this namespace host
 * a backend listing, does it have a `..`, is paste-into meaningful). Per-VOLUME
 * runtime flags (`isReadOnly`, `supportsTrash`, `smbConnectionState`) are NOT
 * here — they live on `VolumeInfo` and layer ON TOP (a specific USB stick is
 * read-only; the "local" KIND is not). See `pane/CLAUDE.md` for the split.
 *
 * ## One classifier, not two
 *
 * `volume-tint.svelte.ts::volumeKindFor` classifies into
 * `'local' | 'smb' | 'mtp' | 'other'` for tinting, collapsing the two virtual
 * kinds + favorites into the untinted `'other'`. `volumeKindOf` here is the
 * SUPERSET: it adds the two virtual kinds as first-class, then DELEGATES to
 * `volumeKindFor` for the real kinds, overriding only its `'other'` fall-through
 * to a documented `'local'` default (real-but-unclassified ⇒ local). The tint
 * classifier keeps its own body and output, so tint stays byte-stable; this
 * module never feeds its `'local'` default back into tinting.
 *
 * ## Q4 (resolved)
 *
 * Capabilities live in this FE per-kind table, NOT per-`VolumeInfo` data from
 * Rust (`VolumeInfo` carries no capability surface today, and the two virtual
 * kinds have no `VolumeInfo` at all). Revisit trigger: if Rust grows a
 * `Volume::capabilities()` surface, this table becomes a fallback/override layer.
 */

import type { LocationCategory } from '$lib/file-explorer/types'
import { volumeKindFor } from './volume-tint.svelte'
import { getVolumes } from '$lib/stores/volume-store.svelte'

/**
 * The closed set of volume kinds. The discriminant — every capability lookup
 * goes kind → record. No `'other'` member: the two virtual kinds plus the three
 * real kinds, nothing else. A real-but-unclassified volume defaults to `'local'`
 * (see `volumeKindOf`), so the kind → table lookup is total.
 */
export type VolumeKind =
  | 'local' // real filesystem volume (root, attached, cloud_drive, main_volume)
  | 'smb' // mounted SMB share (real backend listing, smb path scheme on the share)
  | 'mtp' // connected MTP storage (real backend listing, mtp:// scheme, no system clipboard)
  | 'network' // the synthetic SMB browser virtual volume (host/share list, smb:// namespace)
  | 'search-results' // the snapshot virtual volume (search-results:// namespace, flat result set)

/**
 * What a pane on a given volume KIND can do. A real typed interface (NOT a
 * `Record<string, boolean>` bag): the `kind` field is the discriminant; the
 * rest are the structural capabilities that current guards branch on.
 *
 * Per-VOLUME runtime flags (`isReadOnly`, `supportsTrash`, `smbConnectionState`)
 * are NOT here — they live on `VolumeInfo` and layer on top.
 */
export interface VolumeCapabilities {
  kind: VolumeKind
  /** Real backend directory listing exists (drives the alt-view descriptor, the git/watcher/space/MCP gates). */
  hasBackendListing: boolean
  /** Paste files INTO this pane is meaningful (a real destination folder). False for both virtual kinds. */
  canPasteInto: boolean
  /** Create a child folder/file here (F7 / ⇧F4). False for both virtual kinds. */
  canCreateChild: boolean
  /** Rename the cursor row in-place here (F2). False for the snapshot kind. */
  canRenameInPlace: boolean
  /** This pane can act as the SOURCE of copy/move/delete (snapshot rows are real files ⇒ true). */
  canBeSource: boolean
  /** The system clipboard (⌘C/⌘V) works — needs real local paths. False for mtp + both virtual kinds. */
  supportsSystemClipboard: boolean
  /**
   * Folds ONLY `computeHasParent`'s snapshot rule (`isSearchResultsView ⇒ false`).
   * NOT a complete has-parent answer: the real `hasParent` stays
   * `caps.hasParentRow && currentPath !== '/' && currentPath !== root`, with the
   * two PATH comparisons remaining in `computeHasParent` (a `local` pane at `/`,
   * or any pane on its volume root, has no `..` despite `hasParentRow: true`).
   * False only for the two virtual kinds.
   */
  hasParentRow: boolean
  /** Mirrors pane state to the MCP `PaneState` store (network/search panes are skipped — they have other owners). */
  syncsToMcp: boolean
  /** Path namespace the pane's URLs use. Drives the drop-foreign-listings prefix + display, where converting removes a literal. */
  pathScheme: 'filesystem' | 'smb' | 'mtp' | 'search-results'
}

/**
 * The single source of truth — every cell is justified by a current guard it
 * retires (or is a documented structurally-true-no-guard cell). See
 * `pane/CLAUDE.md` § "Volume capabilities" for the per-cell rationale.
 *
 * Frozen and returned by-reference: `capabilitiesForKind` never allocates (it's
 * hot-ish — FilePane reads it per render).
 */
const CAPABILITY_TABLE: Readonly<Record<VolumeKind, VolumeCapabilities>> = Object.freeze({
  local: Object.freeze({
    kind: 'local',
    hasBackendListing: true,
    canPasteInto: true,
    canCreateChild: true,
    canRenameInPlace: true,
    canBeSource: true,
    supportsSystemClipboard: true,
    hasParentRow: true,
    syncsToMcp: true,
    pathScheme: 'filesystem',
  }),
  smb: Object.freeze({
    kind: 'smb',
    hasBackendListing: true,
    canPasteInto: true,
    canCreateChild: true,
    canRenameInPlace: true,
    canBeSource: true,
    supportsSystemClipboard: true,
    hasParentRow: true,
    syncsToMcp: true,
    pathScheme: 'smb',
  }),
  mtp: Object.freeze({
    kind: 'mtp',
    hasBackendListing: true,
    canPasteInto: true,
    canCreateChild: true,
    canRenameInPlace: true,
    canBeSource: true,
    // MTP has no system clipboard (virtual paths can't go on the OS clipboard);
    // retires `clipboard-operations.ts`'s `startsWith('mtp-')` "Use F5/F6" refusals.
    supportsSystemClipboard: false,
    hasParentRow: true,
    syncsToMcp: true,
    pathScheme: 'mtp',
  }),
  network: Object.freeze({
    kind: 'network',
    // The strictest kind: no listing, no source ops (the host/share list isn't
    // files), no MCP sync (NetworkBrowser owns that push). The paste/create/
    // rename/source `false`s are structurally-true-no-guard cells (a network
    // pane renders NetworkMountView and never reaches the file-list `{#if}`).
    hasBackendListing: false,
    canPasteInto: false,
    canCreateChild: false,
    canRenameInPlace: false,
    canBeSource: false,
    supportsSystemClipboard: false,
    hasParentRow: false,
    syncsToMcp: false,
    pathScheme: 'smb',
  }),
  'search-results': Object.freeze({
    kind: 'search-results',
    // = searchResultsVolumeCapabilities() generalized: canPasteInto/canCreateChild
    // (folds canMkdir+canMkfile)/canRenameInPlace all false; canBeSource (= isSourceOK)
    // true — the snapshot rows are real files.
    hasBackendListing: false,
    canPasteInto: false,
    canCreateChild: false,
    canRenameInPlace: false,
    canBeSource: true,
    supportsSystemClipboard: false,
    hasParentRow: false,
    syncsToMcp: false,
    pathScheme: 'search-results',
  }),
})

/**
 * Pure: pick the kind for a pane. The single classifier (supersedes the tint
 * one). The two virtual ids are checked FIRST, then the real-kind logic is
 * delegated to `volumeKindFor` (the tint classifier), whose `'other'`
 * fall-through (favorites + real-but-unclassified) is overridden to `'local'`
 * so the kind → table lookup is TOTAL.
 *
 * The favorite edge: `volumeKindFor` returns `'other'` for favorites; a favorite
 * is a virtual id pointing at a real path, so the only sane capability set is the
 * real one — `local`. Live panes never sit on a bare favorite id at listing time
 * (the breadcrumb resolves the containing volume), so this is a safety default.
 */
export function volumeKindOf(
  volumeId: string,
  fsType: string | undefined,
  category: LocationCategory | undefined,
): VolumeKind {
  if (volumeId === 'network') return 'network'
  if (volumeId === 'search-results') return 'search-results'
  const tintKind = volumeKindFor(volumeId, fsType, category)
  // `volumeKindFor` returns 'local' | 'smb' | 'mtp' | 'other'. The first three
  // are real kinds in our union; 'other' (favorites + real-but-unclassified)
  // defaults to 'local' — the only sane capability set for a listable volume.
  return tintKind === 'other' ? 'local' : tintKind
}

/** Pure: the capabilities for a kind. Returns the frozen per-kind row (no allocation). */
export function capabilitiesForKind(kind: VolumeKind): VolumeCapabilities {
  return CAPABILITY_TABLE[kind]
}

/**
 * Convenience: classify + look up in one call (the common dispatch/F-bar shape),
 * resolving `fsType`/`category` from the volume store so callers pass only a
 * `volumeId`. The two virtual ids short-circuit in `volumeKindOf` BEFORE the
 * store lookup matters; a stale/missing real id resolves to the `local` default
 * (totality). Never returns `undefined`.
 */
export function capabilitiesFor(volumeId: string, fsType?: string, category?: LocationCategory): VolumeCapabilities {
  // Explicit fsType/category (the FilePane site, which already holds the
  // VolumeInfo) wins; otherwise resolve from the store. Virtual ids don't need
  // either — `volumeKindOf` short-circuits on the id.
  if (fsType === undefined && category === undefined) {
    const info = getVolumes().find((v) => v.id === volumeId)
    return capabilitiesForKind(volumeKindOf(volumeId, info?.fsType, info?.category))
  }
  return capabilitiesForKind(volumeKindOf(volumeId, fsType, category))
}
