/**
 * Volume capabilities — what a pane is allowed to do, and where the answer
 * comes from.
 *
 * ## Rust answers "what can it do", this module classifies "what is it"
 *
 * Every capability a BACKEND can answer arrives as data on `VolumeInfo`
 * (`capabilities`, straight from Rust's `Volume::capabilities()`). This module
 * doesn't re-derive those; it reads them. What stays here is the per-KIND
 * structure Rust has no volume for: the two virtual kinds (`network`,
 * `search-results`) have no `VolumeInfo` at all, `archive` is kind-from-PATH on
 * top of the parent drive's volume, and every real volume needs a default for
 * the window before its backend registers.
 *
 * ❌ Don't publish the backend's own identity (which Rust struct serves a
 * volume) and classify off that: an OS-mounted SMB share that hasn't been
 * upgraded to a direct smb2 session is served by `LocalPosixVolume`, so the
 * backend's answer would be `local` while the share is plainly SMB to the user.
 * KIND is a question about the storage, capability is a question about the
 * backend, and only the second one is Rust's to answer.
 *
 * ## Two layers
 *
 * - The PURE core — `VolumeKind`, `VolumeCapabilities`, the frozen per-kind
 *   table, `volumeKindOf`, `capabilitiesForKind`, `withBackendCapabilities` — is
 *   a leaf: it imports only `volume-tint.svelte` (for the shared real-kind
 *   classifier `volumeKindFor`) and `types.ts`. No `routes/`, no consumers.
 * - The store-reading `capabilitiesFor(volumeId)` resolves the `VolumeInfo` from
 *   the volume store, so callers that hold only a `volumeId` (F-bar, dispatch)
 *   don't replicate the find-in-store dance, and so the backend's published
 *   capabilities get folded in.
 *
 * ## Per-KIND vs per-VOLUME
 *
 * The table carries STRUCTURAL, per-kind capability (can this namespace host a
 * backend listing, does it have a `..`, is paste-into meaningful). The other
 * per-VOLUME runtime flags (`mountIsReadOnly`, `supportsTrash`,
 * `smbConnectionState`) stay on `VolumeInfo` and layer ON TOP.
 *
 * `mountIsReadOnly` and `capabilities.backendCanWrite` sound like one question
 * and are two: whether THIS mount takes writes right now (a read-only `.dmg`, a
 * write-protected stick) versus whether the BACKEND implements mutations at all
 * (`ArchiveVolume` says no, `LocalPosixVolume` says yes). Both combinations
 * happen, which is why each name says whose answer it is.
 *
 * ## One classifier, not two
 *
 * `volume-tint.svelte.ts::volumeKindFor` classifies into
 * `'local' | 'smb' | 'mtp' | 'adb' | 'other'` for tinting, collapsing the two virtual
 * kinds + favorites into the untinted `'other'`. `volumeKindOf` here is the
 * SUPERSET: it adds the two virtual kinds as first-class, then DELEGATES to
 * `volumeKindFor` for the real kinds, overriding only its `'other'` fall-through
 * to a documented `'local'` default (real-but-unclassified ⇒ local). The tint
 * classifier keeps its own body and output, so tint stays byte-stable; this
 * module never feeds its `'local'` default back into tinting.
 */

import type { LocationCategory, VolumeBackendCapabilities, VolumeInfo } from '$lib/file-explorer/types'
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
  | 'adb' // an Android device over ADB (real backend listing, adb:// scheme, no system clipboard)
  | 'network' // the synthetic SMB browser virtual volume (host/share list, smb:// namespace)
  | 'search-results' // the snapshot virtual volume (search-results:// namespace, flat result set)
  | 'archive' // a pane inside a supported archive (kind-from-path; zip is writable, see the row)

/**
 * What a pane on a given volume can do. A real typed interface (NOT a
 * `Record<string, boolean>` bag): the `kind` field is the discriminant.
 *
 * `canWrite` and `canBeSource` are the FOLDED answers — the backend's published
 * `backendCanWrite` / `canExport` laid over the per-kind row whenever the pane
 * sits on a registered volume; the per-kind row
 * is the default for everything Rust has no volume for. The remaining three are
 * per-namespace UI structure Rust has nothing to say about.
 */
export interface VolumeCapabilities {
  kind: VolumeKind
  /** Real backend directory listing exists (drives the alt-view descriptor, the git/watcher/space/MCP gates). */
  hasBackendListing: boolean
  /**
   * Mutations are allowed here: paste INTO, create a child (F7 / ⇧F4), and
   * rename the cursor row in place (F2). ONE flag because it's one question —
   * Rust answers it with one `backendCanWrite`, and splitting it here would
   * be the hand-maintained duplicate all over again.
   */
  canWrite: boolean
  /** This pane can act as the SOURCE of copy/move/delete (snapshot rows are real files ⇒ true). */
  canBeSource: boolean
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
}

/**
 * The per-kind defaults. See `pane/DETAILS.md` § "Volume capabilities" for the
 * per-cell rationale.
 *
 * Frozen and returned by-reference: `capabilitiesForKind` never allocates, and
 * `capabilitiesFor` keeps returning the same row when the backend's published
 * answer already matches it (the case for every ordinary volume).
 */
const CAPABILITY_TABLE: Readonly<Record<VolumeKind, VolumeCapabilities>> = Object.freeze({
  local: Object.freeze({
    kind: 'local',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
  }),
  smb: Object.freeze({
    kind: 'smb',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
  }),
  mtp: Object.freeze({
    kind: 'mtp',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
  }),
  adb: Object.freeze({
    // Same shape as `mtp`: a device-anchored real listing. The transport differs
    // (`adb sync`, a real filesystem), the pane's structure doesn't.
    kind: 'adb',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
  }),
  network: Object.freeze({
    kind: 'network',
    // The strictest kind: no listing, no source ops (the host/share list isn't
    // files), no MCP sync (NetworkBrowser owns that push). The write/source
    // `false`s are structurally-true-no-guard cells (a network pane renders
    // NetworkMountView and never reaches the file-list `{#if}`).
    hasBackendListing: false,
    canWrite: false,
    canBeSource: false,
    hasParentRow: false,
    syncsToMcp: false,
  }),
  'search-results': Object.freeze({
    kind: 'search-results',
    // No folder to write into, but the rows ARE real files, so source ops work.
    hasBackendListing: false,
    canWrite: false,
    canBeSource: true,
    hasParentRow: false,
    syncsToMcp: false,
  }),
  archive: Object.freeze({
    kind: 'archive',
    // A real backend listing (the `ArchiveVolume` lists inner entries like a
    // folder), so the alt-view chain renders the file list, and `..` bubbles out
    // to the zip's containing dir (`hasParentRow`). WRITABLE: rename / mkdir /
    // mkfile / paste run the real managed archive-edit flow (a backend
    // temp+rename rewrite of the whole archive), which is exactly why this row
    // can't come from `ArchiveVolume::capabilities()` — that volume mutates
    // nothing and says so. Zip is the only mutable format; a path inside a tar
    // or 7z gets `READ_ONLY_ARCHIVE` instead.
    // `canBeSource: true` — copying files OUT stays a headline feature.
    // `syncsToMcp: true` — the listing is real; MCP reports the parent drive id
    // plus the full `…/foo.zip/inner` path, so agents navigate by path.
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
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
  // `volumeKindFor` returns 'local' | 'smb' | 'mtp' | 'adb' | 'other'. The first four
  // are real kinds in our union; 'other' (favorites + real-but-unclassified)
  // defaults to 'local' — the only sane capability set for a listable volume.
  return tintKind === 'other' ? 'local' : tintKind
}

/** Pure: the per-kind defaults for a kind. Returns the frozen row (no allocation). */
export function capabilitiesForKind(kind: VolumeKind): VolumeCapabilities {
  return CAPABILITY_TABLE[kind]
}

/**
 * Pure: lay the backend's published answer over the per-kind defaults.
 *
 * `published` is absent for everything Rust has no volume for (the two virtual
 * kinds, a favorite id, a real volume discovery found before its backend
 * registered), and the defaults stand. When it IS present it wins — that's the
 * volume itself talking. Returns the frozen row unchanged when the two already
 * agree, which is the case for every ordinary volume, so the hot path stays
 * allocation-free.
 */
export function withBackendCapabilities(
  row: VolumeCapabilities,
  published: VolumeBackendCapabilities | null | undefined,
): VolumeCapabilities {
  if (!published) return row
  if (published.backendCanWrite === row.canWrite && published.canExport === row.canBeSource) return row
  return Object.freeze({ ...row, canWrite: published.backendCanWrite, canBeSource: published.canExport })
}

/**
 * The capabilities for a volume id: classify the kind, then fold in whatever the
 * backend published.
 *
 * The two virtual ids short-circuit in `volumeKindOf` BEFORE the store lookup
 * matters; a stale/missing real id resolves to the `local` default (totality).
 * Never returns `undefined`.
 */
export function capabilitiesFor(volumeId: string): VolumeCapabilities {
  const info: VolumeInfo | undefined = getVolumes().find((v) => v.id === volumeId)
  const row = capabilitiesForKind(volumeKindOf(volumeId, info?.fsType, info?.category))
  return withBackendCapabilities(row, info?.capabilities)
}

/**
 * The supported archive-name SUFFIXES, MIRRORING the backend's `format_for_name`
 * (`crates/cmdr-fs/src/archive_format.rs`). Kept in lockstep — the FE does the
 * cheap suffix pre-filter, the backend stat- and magic-confirms on actual
 * navigation. Suffix-based (not just the last `.ext`) so `.tar.gz` matches while
 * a bare `.gz` doesn't. Longest-first so `.tar.gz` wins over `.tar`.
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
 * ❌ The archive branch deliberately does NOT fold in the parent volume's
 * published capabilities: those answer for the drive, and the pane is inside a
 * file on it. Zip is writable (the managed archive-edit flow); tar and 7z are
 * browse + extract only, so a path inside a non-zip archive gets
 * `READ_ONLY_ARCHIVE`. Which format is decided by the FIRST archive boundary
 * segment (leftmost wins, matching the backend), so a nested `foo.tar/bar.zip/…`
 * is read-only (the outer tar governs).
 */
export function capabilitiesForPane(volumeId: string, path: string | undefined): VolumeCapabilities {
  const boundarySegment = path === undefined ? undefined : path.split('/').find(hasSupportedArchiveExtension)
  if (boundarySegment !== undefined) {
    return isWritableArchiveName(boundarySegment) ? CAPABILITY_TABLE.archive : READ_ONLY_ARCHIVE
  }
  return capabilitiesFor(volumeId)
}

/**
 * The read-only archive capability (tar / 7z): the `archive` row with `canWrite`
 * turned OFF. Same `kind: 'archive'` (tint, breadcrumb, and MCP are identical to
 * a zip pane); only mutation is gated off. `canBeSource` stays true so copying
 * files OUT — the headline read feature — still works.
 */
const READ_ONLY_ARCHIVE: VolumeCapabilities = Object.freeze({
  ...CAPABILITY_TABLE.archive,
  canWrite: false,
})
