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
 * `search-results`) have no `VolumeInfo` at all, the two ROUTED kinds (`archive`,
 * `git-portal`) are kind-from-PATH on top of the parent drive's volume, and every
 * real volume needs a default for the window before its backend registers.
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
 *   capabilities get folded in. `capabilitiesForPane(volumeId, path)` sits on
 *   top and adds the two routed kinds, which is where the git-portal toggle and
 *   the archive-suffix table are read.
 *
 * ## Per-KIND vs per-VOLUME
 *
 * The table carries STRUCTURAL, per-kind capability (can this namespace host a
 * backend listing, does it have a `..`, is paste-into meaningful). The other
 * per-VOLUME runtime flags (`mountIsReadOnly`, `supportsTrash`,
 * `connectionState`) stay on `VolumeInfo` and layer ON TOP.
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
 * `'local' | 'smb' | 'sftp' | 'webdav' | 'mtp' | 'adb' | 'other'` for tinting, collapsing the two virtual
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
import { isVirtualGitPath } from '../git/path-detection'
import { getShowVirtualGitPortal } from '$lib/settings/reactive-settings.svelte'
import { isPlainFilesystemPath } from '$lib/path/canonical'

/**
 * The closed set of volume kinds. The discriminant — every capability lookup
 * goes kind → record. No `'other'` member: the two virtual kinds plus the six
 * real kinds plus the two routed ones, nothing else.
 *
 * ❗ **Nothing switches exhaustively over this union.** Every consumer is a
 * positive-list comparison (`kind === 'mtp' || kind === 'adb'`,
 * `kind === 'smb'`), so adding a member compiles clean everywhere and the new
 * kind silently falls out of each list. Adding one means walking the consumers:
 * `pane/clipboard-operations.ts`, `volume-tint.svelte.ts`,
 * `search/search-target-volume.ts`, and `open-terminal/terminal-target.ts`. A real-but-unclassified
 * volume defaults to `'local'` (see `volumeKindOf`), so the kind → table lookup
 * is total.
 *
 * `archive` and `git-portal` are KIND-FROM-PATH, not kind-from-id: a pane whose
 * PATH crosses a supported archive (`pathCrossesArchiveBoundary`) or one of the six
 * virtual `.git` categories (`isVirtualGitPath`) takes that kind regardless of
 * its `volumeId`, which stays the parent drive (the tab keeps ONE id). This union
 * is DELIBERATELY WIDER than the tint union in `volume-tint.svelte.ts`: both
 * routed panes show the PARENT drive's tint (they live on that drive), so the
 * two are capability kinds only, never tint kinds.
 */
export type VolumeKind =
  | 'local' // real filesystem volume (root, attached, cloud_drive, main_volume)
  | 'smb' // mounted SMB share (real backend listing, smb path scheme on the share)
  | 'sftp' // an SFTP server (real backend listing, sftp:// scheme, no system clipboard, no terminal)
  | 'webdav' // a WebDAV server (real backend listing, webdav:// scheme, no system clipboard, no terminal)
  | 'mtp' // connected MTP storage (real backend listing, mtp:// scheme, no system clipboard)
  | 'adb' // an Android device over ADB (real backend listing, adb:// scheme, no system clipboard)
  | 'network' // the synthetic SMB browser virtual volume (host/share list, smb:// namespace)
  | 'search-results' // the snapshot virtual volume (search-results:// namespace, flat result set)
  | 'archive' // a pane inside a supported archive (kind-from-path; zip is writable, see the row)
  | 'git-portal' // a pane inside one of the virtual `.git` category trees (kind-from-path, read-only)

/**
 * What a pane on a given volume can do. A real typed interface (NOT a
 * `Record<string, boolean>` bag): the `kind` field is the discriminant.
 *
 * `canWrite`, `canBeSource`, and `canBeIndexed` are the FOLDED answers — the
 * backend's published `backendCanWrite` / `canExport` / `canBeIndexed` laid over
 * the per-kind row whenever the pane sits on a registered volume; the per-kind
 * row is the default for everything Rust has no volume for. The remaining three
 * are per-namespace UI structure Rust has nothing to say about.
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
  /**
   * FilePane mirrors this pane's state to the MCP `PaneState` store. False only
   * for `network`, where `ServersHub` owns the push and FilePane's would
   * clobber its host list. Every other kind mirrors, the search-results snapshot
   * included: it's a real pane an agent moves the cursor in and deletes from, and
   * a pane that pushes nothing leaves the store describing wherever it came from.
   */
  syncsToMcp: boolean
  /**
   * A drive index can be turned on here, so the switcher offers its index
   * affordances (`navigation/drive-index-manager.svelte.ts::isDriveRow`). Rust's
   * `BackendKind::can_be_indexed` is the one decider; this row only answers
   * before a backend registers, like a phone's row clicked before it's dialed.
   */
  canBeIndexed: boolean
  /**
   * FilePane polls whether this pane's folder still exists (`deleted-dir-poll.ts`),
   * covering FSEvents' blind spot: macOS doesn't report a watched folder's own
   * deletion. So it's true only where the folder lives on a filesystem the Mac
   * itself mounts and watches (`local`, `smb`, and an `archive` sitting on one).
   * Read it through `paneFolderIsPolledForDeletion`, which adds the path half.
   */
  pollsForDeletedFolder: boolean
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
    canBeIndexed: true,
    pollsForDeletedFolder: true,
  }),
  smb: Object.freeze({
    kind: 'smb',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
    // Both an smb2 session and an OS-mounted share: the index walks either.
    canBeIndexed: true,
    // The share stays OS-mounted at `/Volumes/…`, so the Mac's own stat answers.
    pollsForDeletedFolder: true,
  }),
  sftp: Object.freeze({
    // A server: a real backend listing over a session Cmdr owns, with `..` and a
    // sort like any folder. Split from `smb` because there is no OS mount behind
    // it: its paths carry an `sftp://` scheme the system clipboard and a shell
    // can't use, which `clipboard-operations.ts` and `canOpenTerminalIn` gate on.
    // Write and export come from the backend's published answer, laid over these.
    kind: 'sftp',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
    // No drive index: its `sftp://` root is nothing the index's walkers can read.
    canBeIndexed: false,
    // No OS mount, so no FSEvents blind spot to cover. Whether a server pane should
    // notice its folder deleted on the server is a separate question nobody polls for.
    pollsForDeletedFolder: false,
  }),
  webdav: Object.freeze({
    // The `sftp` row, for the same reasons: a session-backed listing with no OS
    // mount behind it.
    kind: 'webdav',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
    canBeIndexed: false,
    pollsForDeletedFolder: false,
  }),
  mtp: Object.freeze({
    kind: 'mtp',
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
    canBeIndexed: true,
    // A device path the Mac can't stat. An unplugged phone is `mtp-disconnect-watch`'s.
    pollsForDeletedFolder: false,
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
    // Indexed like an MTP phone. This row is what answers for a phone's row
    // BEFORE it's dialed, which is when the first-connect prompt fires.
    canBeIndexed: true,
    pollsForDeletedFolder: false,
  }),
  network: Object.freeze({
    kind: 'network',
    // The strictest kind: no listing, no source ops (the host/share list isn't
    // files), no MCP sync (ServersHub owns that push). The write/source
    // `false`s are structurally-true-no-guard cells (a network pane renders
    // NetworkMountView and never reaches the file-list `{#if}`).
    hasBackendListing: false,
    canWrite: false,
    canBeSource: false,
    hasParentRow: false,
    syncsToMcp: false,
    canBeIndexed: false,
    pollsForDeletedFolder: false,
  }),
  'search-results': Object.freeze({
    kind: 'search-results',
    // No folder to write into, but the rows ARE real files, so source ops work.
    hasBackendListing: false,
    canWrite: false,
    canBeSource: true,
    hasParentRow: false,
    // `syncsToMcp: true` despite having no backend listing: the rows come from the
    // frontend snapshot instead (`snapshot-mcp-rows.ts`), and MCP's copy/move/delete
    // gate reasons on this pane's state, so a pane that pushed nothing left the
    // store describing the directory it came from.
    syncsToMcp: true,
    canBeIndexed: false,
    // No folder behind the namespace, so nothing to poll.
    pollsForDeletedFolder: false,
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
    // `canBeIndexed: false` — a view inside a drive, which is indexed as itself.
    // `pollsForDeletedFolder: true` — deleting the `.zip` is the case to notice,
    // and the path half of `paneFolderIsPolledForDeletion` keeps a zip on a phone
    // or a server out.
    hasBackendListing: true,
    canWrite: true,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
    canBeIndexed: false,
    pollsForDeletedFolder: true,
  }),
  'git-portal': Object.freeze({
    kind: 'git-portal',
    // A pane inside one of the six virtual `.git` category trees (`branches/`,
    // `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/`). The archive
    // row's shape, with mutation off: `GitPortalVolume` lists snapshot entries
    // like a folder (`hasBackendListing`), `..` walks back out by plain path
    // arithmetic (`hasParentRow`), and the rows are real content the transfer
    // reads through the portal, so copying OUT works (`canBeSource`).
    // `canWrite: false` because a snapshot is git history, not a directory:
    // every mutation method on the volume keeps the trait's `NotSupported`.
    // `syncsToMcp: true` — the listing is real; MCP reports the parent drive id
    // plus the full `…/.git/branches/main/…` path, so agents navigate by path.
    // `pollsForDeletedFolder: false` — a snapshot folder never exists on disk, so
    // the Mac's stat would call it gone and evict the user back to `.git/`. The git
    // watcher keeps these listings fresh instead.
    hasBackendListing: true,
    canWrite: false,
    canBeSource: true,
    hasParentRow: true,
    syncsToMcp: true,
    canBeIndexed: false,
    pollsForDeletedFolder: false,
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
  // `volumeKindFor` returns 'local' | 'smb' | 'sftp' | 'webdav' | 'mtp' | 'adb' |
  // 'other'. The first six are real kinds in our union; 'other' (favorites +
  // real-but-unclassified) defaults to 'local' — the only sane capability set for
  // a listable volume.
  return tintKind === 'other' ? 'local' : tintKind
}

/** Pure: the per-kind defaults for a kind. Returns the frozen row (no allocation). */
export function capabilitiesForKind(kind: VolumeKind): VolumeCapabilities {
  return CAPABILITY_TABLE[kind]
}

/**
 * Whether a pane on this kind hands out ROWS that are ordinary paths on the OS
 * filesystem. What "Share…" needs: the macOS share sheet takes file URLs, and a
 * URL for a row with no file behind it produces a sheet that can send nothing.
 *
 * `local` and `smb` yes (both keep a real `/Volumes/…` mount alive, the same
 * reading Rust takes with `Volume::paths_are_os_visible()`), and so does
 * `search-results` — that's the one row where this parts company with
 * `canOpenTerminalIn`, which asks about the pane's own FOLDER and gets `false`
 * from a snapshot that has none. `mtp` / `adb` stream from a device, `archive`
 * and `git-portal` rows are synthesized from a container, and `network` lists
 * hosts rather than files.
 *
 * ❌ Never a test on the path string: an archive-inner path looks exactly like a
 * folder path, and a share whose mount went away still looks local.
 */
export function paneRowsAreOsVisible(kind: VolumeKind): boolean {
  return kind === 'local' || kind === 'smb' || kind === 'search-results'
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
  if (
    published.backendCanWrite === row.canWrite &&
    published.canExport === row.canBeSource &&
    published.canBeIndexed === row.canBeIndexed
  ) {
    return row
  }
  return Object.freeze({
    ...row,
    canWrite: published.backendCanWrite,
    canBeSource: published.canExport,
    canBeIndexed: published.canBeIndexed,
  })
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
  return info ? capabilitiesForInfo(info) : capabilitiesForKind(volumeKindOf(volumeId, undefined, undefined))
}

/**
 * The same answer as `capabilitiesFor`, for a caller already holding the volume
 * row: a site walking the volume list (the switcher's index affordances) has the
 * row in hand, and a lookup by id would be a second pass over the same list.
 */
export function capabilitiesForInfo(info: VolumeInfo): VolumeCapabilities {
  const row = capabilitiesForKind(volumeKindOf(info.id, info.fsType, info.category))
  return withBackendCapabilities(row, info.capabilities)
}

/**
 * Whether ONE row hands out a path the OS filesystem knows: the gate behind
 * "Share…", which needs a file URL a share service can actually read.
 *
 * Three questions, and each one needs a different input, which is why this can't
 * collapse into a single kind lookup:
 *
 * 1. The VOLUME (`paneRowsAreOsVisible`) rules out phones and the host list.
 * 2. The ROW's path rules out an archive's insides. `capabilitiesForPane` is the
 *    wrong tool here: it uses the WIDE archive check, which would call the `.zip`
 *    file itself unshareable, and sharing a freshly-made archive is the point.
 * 3. The row's path again, for the virtual `.git` portal, and only while the
 *    portal is switched ON (with it off the same path is whatever is on disk).
 */
export function rowIsOsVisible(volumeId: string, rowPath: string): boolean {
  if (!paneRowsAreOsVisible(capabilitiesFor(volumeId).kind)) return false
  if (pathInsideArchive(rowPath)) return false
  return !(getShowVirtualGitPortal() && isVirtualGitPath(rowPath))
}

/**
 * Whether the pane's folder is polled for being deleted behind its back, the gate
 * `deleted-dir-poll.ts` runs on.
 *
 * Two halves, and neither covers the other:
 *
 * 1. The pane's KIND (`pollsForDeletedFolder`, through `capabilitiesForPane`, so
 *    the `.git` portal's snapshot folders are out).
 * 2. The PATH is one the Mac can stat at all. This is what holds once the pane's
 *    row is gone: an unplugged phone's pane keeps its `adb://` path, and a removed
 *    server's stale id classifies as `local`, so without it the boot disk answers
 *    "gone" for a path it can't see and the walk-up re-lists a vanished place.
 *    Same reasoning as the snapshot clipboard's scheme gate.
 */
export function paneFolderIsPolledForDeletion(volumeId: string, path: string): boolean {
  return isPlainFilesystemPath(path) && capabilitiesForPane(volumeId, path).pollsForDeletedFolder
}

/**
 * The supported archive-name SUFFIXES, MIRRORING the backend's `format_for_name`
 * (`crates/cmdr-fs/src/archive_format.rs`). Kept in lockstep — the FE does the
 * cheap suffix pre-filter, the backend stat- and magic-confirms on actual
 * navigation. Suffix-based (not just the last `.ext`) so `.tar.gz` matches while
 * a bare `.gz` doesn't. Longest-first so `.tar.gz` wins over `.tar`.
 */
export const SUPPORTED_ARCHIVE_SUFFIXES: readonly string[] = [
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
  // Zip containers that are a DOCUMENT or an app package rather than an archive
  // the user assembled. Browsable, never writable — see below.
  '.docx',
  '.xlsx',
  '.pptx',
  '.jar',
  '.apk',
]

/**
 * The WRITABLE archive suffixes: only zip. tar and 7z are browse + extract only,
 * so a pane inside one gets the read-only archive capability. Mirrors the backend
 * write chokepoint (`archive_edit::ensure_zip_writable`).
 *
 * A DOCUMENT container (`.docx`, `.jar`, …) is absent for a stronger reason than
 * tar and 7z are: those simply have no mutator, while a `.docx` IS a zip and the
 * mutator would happily rewrite one. Letting a user rename or delete parts while
 * wandering inside a Word file hands them a corrupt document, so the backend
 * refuses it by TYPE (`ArchiveFormat::Ooxml` never satisfies `ensure_zip_writable`)
 * and this list keeps the UI honest about it. ❌ Never add one here.
 */
export const WRITABLE_ARCHIVE_SUFFIXES: readonly string[] = ['.zip']

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
 * Whether `path` is AT or inside a supported archive — the WIDE half of the pair,
 * a pure extension-only string check (NO I/O) mirroring the backend's
 * `path_crosses_archive_boundary`: ANY path component (not just the last)
 * carrying a supported archive extension crosses. `/a/foo.zip` (the archive root)
 * and `/a/foo.zip/inner` both return true; `/a` (a plain folder that merely
 * CONTAINS `foo.zip`) does not.
 *
 * This is the ENTER-IT question, so it's what a site gating on the PANE's path
 * wants: a pane sitting at `/a/foo.zip` is showing the archive's contents, and
 * a git lookup, a disk-space query, or a write-capability row must treat it as
 * such. For a site that operates ON a path — preview it, move it, rename it —
 * reach for `pathInsideArchive` instead: the `.zip` file itself is an ordinary
 * file there.
 *
 * This is a lower bound the backend corrects: a real directory literally named
 * `foo.zip`, or a mislabeled non-archive file, is NOT decidable here (it needs a
 * stat + magic sniff). The FE uses it only for read-only capability gating, where
 * a false "read-only" is safe (the backend rejects a genuinely writable-target
 * mistake) and a missed one is caught by the backend `ReadOnlyDevice` net.
 */
export function pathCrossesArchiveBoundary(path: string): boolean {
  return path.split('/').some((segment) => hasSupportedArchiveExtension(segment))
}

/**
 * Whether `path` points at something strictly INSIDE a supported archive — the
 * NARROW half, mirroring the backend's `path_is_inside_archive`. True only when
 * the archive boundary is followed by a non-empty inner path:
 * `/a/foo.zip/inner` yes, `/a/foo.zip` (and `/a/foo.zip/`) no.
 *
 * The distinction is load-bearing, in the backend's own words: an archive-inner
 * path has no real file behind it, while the `.zip` file ITSELF is a regular file
 * that must be copied, moved, renamed, previewed, and Quick Looked exactly like
 * any other. Sites that operate ON a path use this one; sites that navigate INTO
 * a path use `pathCrossesArchiveBoundary`.
 *
 * Gets sharper the more zip-container formats Cmdr browses: with `.docx` a
 * supported suffix, the wide check here would refuse Quick Look on every Word
 * document, which is what this half exists to prevent.
 */
export function pathInsideArchive(path: string): boolean {
  const segments = path.split('/')
  const boundary = segments.findIndex((segment) => hasSupportedArchiveExtension(segment))
  if (boundary === -1) return false
  // A trailing slash leaves an empty segment, which is still the archive ROOT —
  // so ask for a non-empty inner component rather than just a longer array.
  return segments.slice(boundary + 1).some((segment) => segment.length > 0)
}

/**
 * The display name of the archive a path is at or inside: the FIRST path segment
 * carrying a supported archive extension (leftmost wins, matching the backend's
 * boundary resolution and `pathCrossesArchiveBoundary`), so `/a/photos.zip/inner/x.jpg`
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
 * The real folder on disk that CONTAINS the archive a path is at or inside: the
 * directory holding the FIRST archive-extension segment, so
 * `/a/b/photos.zip/inner/x.jpg` and `/a/b/photos.zip` both return `/a/b`. The
 * leftmost-wins rule matches `pathCrossesArchiveBoundary` and the backend's boundary
 * resolution, so a nested `foo.tar/bar.zip/…` resolves against the outer tar.
 *
 * Returns `'/'` when the archive sits at the filesystem root, and the path
 * unchanged when no segment is an archive (a caller should only reach here for an
 * in-archive path, but the fallback keeps it total). Pure, no I/O.
 */
export function folderContainingArchive(path: string): string {
  const segments = path.split('/')
  const boundary = segments.findIndex((segment) => hasSupportedArchiveExtension(segment))
  if (boundary === -1) return path
  const parent = segments.slice(0, boundary).join('/')
  return parent === '' ? '/' : parent
}

/**
 * Capabilities for a PANE, resolving the kind from BOTH the volume id and the
 * path (kind-from-path). A path inside a supported archive is the `archive` kind
 * regardless of the parent-drive `volumeId`; otherwise this defers to
 * `capabilitiesFor`. This is the entry point every write-guard site uses so an
 * archive pane — whose `volumeId` is the WRITABLE parent drive — is gated by the
 * ARCHIVE row (zip mutation), not the parent drive's row.
 *
 * ❌ Neither routed branch folds in the parent volume's published capabilities:
 * those answer for the drive, and the pane is inside something ON it. Zip is
 * writable (the managed archive-edit flow); tar and 7z are browse + extract only,
 * so a path inside a non-zip archive gets `READ_ONLY_ARCHIVE`. Which format is
 * decided by the FIRST archive boundary segment (leftmost wins, matching the
 * backend), so a nested `foo.tar/bar.zip/…` is read-only (the outer tar governs).
 *
 * The second routed kind is the virtual `.git` portal. `isVirtualGitPath` is the
 * same LEXICAL test the backend routes on (`.git/<category>/…` for the six
 * categories), gated on the live portal toggle, because with the portal off
 * `resolve` routes nothing and `.git/branches/` is whatever sits on disk. Real
 * files under `.git/` (`config`, `HEAD`, `refs/heads/main`) are not portal paths
 * and keep the parent volume's full row: they stay editable, renamable, and
 * deletable, which is a constraint the backend defends too.
 */
export function capabilitiesForPane(volumeId: string, path: string | undefined): VolumeCapabilities {
  if (path === undefined) return capabilitiesFor(volumeId)
  const boundarySegment = path.split('/').find(hasSupportedArchiveExtension)
  if (boundarySegment !== undefined) {
    return isWritableArchiveName(boundarySegment) ? CAPABILITY_TABLE.archive : READ_ONLY_ARCHIVE
  }
  if (getShowVirtualGitPortal() && isVirtualGitPath(path)) return CAPABILITY_TABLE['git-portal']
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
