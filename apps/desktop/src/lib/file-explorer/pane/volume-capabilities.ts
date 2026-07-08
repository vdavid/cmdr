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
 * real kinds plus `archive`, nothing else. A real-but-unclassified volume
 * defaults to `'local'` (see `volumeKindOf`), so the kind → table lookup is total.
 *
 * `archive` is KIND-FROM-PATH, not kind-from-id: a pane whose PATH crosses a
 * supported archive (`pathInsideArchive`) is an `archive` kind regardless of its
 * `volumeId`, which stays the parent drive (the tab keeps ONE id). This union is
 * DELIBERATELY WIDER than the tint union in `volume-tint.svelte.ts`: an archive
 * pane shows the PARENT drive's tint (it lives on that drive), so `archive` is a
 * capability kind only, never a tint kind.
 */
export type VolumeKind =
  | 'local' // real filesystem volume (root, attached, cloud_drive, main_volume)
  | 'smb' // mounted SMB share (real backend listing, smb path scheme on the share)
  | 'mtp' // connected MTP storage (real backend listing, mtp:// scheme, no system clipboard)
  | 'network' // the synthetic SMB browser virtual volume (host/share list, smb:// namespace)
  | 'search-results' // the snapshot virtual volume (search-results:// namespace, flat result set)
  | 'archive' // a pane inside a supported archive (kind-from-path; zip is writable, see the row)

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
  /**
   * Path namespace the pane's URLs use. NOTE: nearly vestigial — the only runtime
   * reader is `clipboard-operations.ts` (a `!== 'search-results'` gate, which
   * reads the PARENT volume's row via id-only `capabilitiesFor`, never the archive
   * row). The drop-foreign-listings prefix is computed from an id compare in
   * `navigate.ts`, not from this field. So the `archive` row's `'filesystem'` is
   * HARMLESS today even for an SMB/MTP-backed archive. But it's a latent trap:
   * `capabilitiesForPane` returns the frozen `archive` row wholesale, so a remote-
   * backed archive pane reports `'filesystem'` regardless of its parent scheme. Any
   * FUTURE consumer routing clipboard/transfer/display off `caps.pathScheme` for an
   * archive pane must NOT trust this — blend in the parent volume's scheme in
   * `capabilitiesForPane` first.
   */
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
    // The original search-results capability seed, generalized: canPasteInto/canCreateChild
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
  archive: Object.freeze({
    kind: 'archive',
    // A real backend listing (the `ArchiveVolume` lists inner entries like a
    // folder), so the alt-view chain renders the file list, and `..` bubbles out
    // to the zip's containing dir (`hasParentRow`). WRITABLE: the three write
    // flags are true, so rename/mkdir/mkfile/paste run the real managed
    // archive-edit flow (backend temp+rename rewrite). Zip is the only supported
    // archive format today and it's mutable; when M7 adds browse-only formats
    // (tar/7z), `capabilitiesForPane` must return a read-only archive variant for
    // a path whose boundary is a non-writable format (see its comment).
    // `canBeSource: true` — copying files OUT stays a headline feature.
    // No system clipboard: an archive-inner path isn't an OS-resolvable local
    // path, so ⌘C/⌘V can't carry it (F5/F6 is the supported path in AND out).
    // `syncsToMcp: true` — the listing is real; MCP reports the parent drive id
    // plus the full `…/foo.zip/inner` path, so agents navigate by path.
    hasBackendListing: true,
    canPasteInto: true,
    canCreateChild: true,
    canRenameInPlace: true,
    canBeSource: true,
    supportsSystemClipboard: false,
    hasParentRow: true,
    syncsToMcp: true,
    pathScheme: 'filesystem',
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

/**
 * The supported archive-name SUFFIXES, MIRRORING the backend's `format_for_name`
 * (`backends/archive/format.rs`). Kept in lockstep — the FE does the cheap
 * suffix pre-filter, the backend stat- and magic-confirms on actual navigation.
 * Suffix-based (not just the last `.ext`) so `.tar.gz` matches while a bare `.gz`
 * doesn't. Longest-first so `.tar.gz` wins over `.tar`.
 */
const SUPPORTED_ARCHIVE_SUFFIXES: readonly string[] = [
  '.tar.gz',
  '.tar.bz2',
  '.tar.xz',
  '.tar.zst',
  '.tgz',
  '.tbz2',
  '.tbz',
  '.txz',
  '.tzst',
  '.tar',
  '.zip',
  '.7z',
]

/**
 * The WRITABLE archive suffixes: only zip. tar and 7z are browse + extract only,
 * so a pane inside one gets the read-only archive capability. Mirrors the backend
 * write chokepoint (`archive_edit::ensure_zip_writable`).
 */
const WRITABLE_ARCHIVE_SUFFIXES: readonly string[] = ['.zip']

/** Whether `name` ends with `suffix` and has a real stem before it. */
function nameHasSuffix(name: string, suffix: string): boolean {
  const lower = name.toLowerCase()
  return lower.endsWith(suffix) && lower.length > suffix.length
}

/**
 * True if `name`'s suffix is a supported archive format (case-insensitive).
 * Mirrors the backend's `has_supported_archive_extension`.
 */
function hasSupportedArchiveExtension(name: string): boolean {
  return SUPPORTED_ARCHIVE_SUFFIXES.some((s) => nameHasSuffix(name, s))
}

/** True if `name` is a WRITABLE archive (zip) — tar/7z return false. */
function isWritableArchiveName(name: string): boolean {
  return WRITABLE_ARCHIVE_SUFFIXES.some((s) => nameHasSuffix(name, s))
}

/**
 * Whether `path` is at or inside a supported archive — a pure, extension-only
 * string check (NO I/O), mirroring the backend's `archive_boundary_candidate`:
 * ANY path component (not just the last) carrying a supported archive extension
 * crosses the boundary. `/a/foo.zip` (the archive root) and `/a/foo.zip/inner`
 * both return true; `/a` (a plain folder that merely CONTAINS `foo.zip`) does not.
 *
 * This is a lower bound the backend corrects: a real directory literally named
 * `foo.zip`, or a mislabeled non-archive file, is NOT decidable here (it needs a
 * stat + magic sniff). The FE uses it only for read-only capability gating, where
 * a false "read-only" is safe (the backend rejects a genuinely writable-target
 * mistake) and a missed one is caught by the backend `ReadOnlyDevice` net.
 */
export function pathInsideArchive(path: string): boolean {
  return path.split('/').some((segment) => hasSupportedArchiveExtension(segment))
}

/**
 * The display name of the archive a path is at or inside: the FIRST path segment
 * carrying a supported archive extension (leftmost wins, matching the backend's
 * boundary resolution and `pathInsideArchive`), so `/a/photos.zip/inner/x.jpg`
 * returns `photos.zip`. Falls back to the path's basename when no segment is an
 * archive (a caller should only reach here for an in-archive path, but the
 * fallback keeps it total). Pure, no I/O.
 */
export function archiveNameFromPath(path: string): string {
  const segments = path.split('/').filter((s) => s.length > 0)
  const archiveSegment = segments.find((s) => hasSupportedArchiveExtension(s))
  if (archiveSegment) return archiveSegment
  return segments.length > 0 ? segments[segments.length - 1] : path
}

/**
 * Capabilities for a PANE, resolving the kind from BOTH the volume id and the
 * path (kind-from-path). A path inside a supported archive is the `archive` kind
 * regardless of the parent-drive `volumeId`; otherwise this defers to
 * `capabilitiesFor`. This is the entry point every write-guard site uses so an
 * archive pane — whose `volumeId` is the WRITABLE parent drive — is gated by the
 * ARCHIVE row (zip mutation), not the parent drive's row.
 *
 * Zip is writable (the managed archive-edit flow); tar and 7z are browse +
 * extract only. So a path inside a NON-zip archive gets `READ_ONLY_ARCHIVE` (the
 * three write flags off, `canBeSource` on so copy-OUT still works) — the
 * kind-from-path split the M4b seam anticipated. Which format is decided by the
 * FIRST archive boundary segment (leftmost wins, matching the backend), so a
 * nested `foo.tar/bar.zip/…` is read-only (the outer tar governs).
 */
export function capabilitiesForPane(
  volumeId: string,
  path: string | undefined,
  fsType?: string,
  category?: LocationCategory,
): VolumeCapabilities {
  const boundarySegment = path === undefined ? undefined : path.split('/').find(hasSupportedArchiveExtension)
  if (boundarySegment !== undefined) {
    return isWritableArchiveName(boundarySegment) ? CAPABILITY_TABLE.archive : READ_ONLY_ARCHIVE
  }
  return capabilitiesFor(volumeId, fsType, category)
}

/**
 * The read-only archive capability (tar / 7z): the `archive` row with the three
 * write flags turned OFF. Same `kind: 'archive'` (tint, breadcrumb, and MCP are
 * identical to a zip pane); only mutation is gated off. `canBeSource` stays true
 * so copying files OUT — the headline read feature — still works.
 */
const READ_ONLY_ARCHIVE: VolumeCapabilities = Object.freeze({
  ...CAPABILITY_TABLE.archive,
  canPasteInto: false,
  canCreateChild: false,
  canRenameInPlace: false,
})
