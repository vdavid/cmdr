// File actions: open, reveal, preview, and context menu commands

import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import {
  commands,
  type DriveItemLinks,
  type ShareLinkExpiry,
  type VolumeError,
  type EditorOpenReport,
  type GetInfoError,
  type OpenInEditorError,
  type OpenTerminalError,
  type OpenTerminalOutcome,
  type TerminalAppList,
  type TextEditorList,
  type TimedOut,
} from '$lib/ipc/bindings'
import { TypedFailure } from '$lib/ipc/typed-failure'
import type { SameKindTarget } from '$lib/file-explorer/pane/select-same-kind'
import { throwIpcError } from './ipc-types'

export type {
  DriveItemLinks,
  EditorOpenOutcome,
  EditorOpenReport,
  GetInfoError,
  OpenInEditorError,
  OpenTerminalError,
  OpenTerminalOutcome,
  TerminalApp,
  TerminalAppList,
  TextEditorApp,
  TextEditorList,
} from '$lib/ipc/bindings'

/**
 * Opens a file with the system's default application.
 *
 * Routes through the backend `openPath` command (not the opener plugin) so the
 * `playwright-e2e` build can record the open instead of launching an external
 * app. Otherwise the E2E suite floods the desktop with orphan TextEdit/Preview
 * windows it can't close. See `src-tauri/src/commands/file_actions.rs`.
 * @param path - Path to the file to open.
 */
export async function openFile(path: string): Promise<void> {
  const res = await commands.openPath(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Opens a URL in the system's default browser.
 * @param url - URL to open (like "https://getcmdr.com/renew")
 */
export async function openExternalUrl(url: string): Promise<void> {
  await openUrl(url)
}

/**
 * What the PANE contributes to a file context menu, as opposed to the file that was
 * right-clicked. One object rather than three trailing booleans and a string: they
 * all come from one pane read, and positional flags of the same type are exactly
 * what silently binds to the wrong slot. Every field defaults to the most
 * restrictive answer, so a surface that can't answer says nothing.
 */
export interface PaneContextMenuFacts {
  /**
   * Hides Rename and New folder. `true` for a right-click inside a virtual pane
   * that isn't a real directory (the search-results snapshot pane; see
   * `apps/desktop/src/lib/search/capabilities.ts`).
   */
  restrictDestinationActions?: boolean
  /** Whether this menu is over a search result that can be opened in its containing folder. */
  canShowInFolder?: boolean
  /**
   * The pane's listing id, so a Finder-tag color click can refresh that listing's
   * cache after writing. Omit for a virtual pane with no normal listing; the tag
   * still writes to disk.
   */
  listingId?: string
  /**
   * Whether "Open terminal here" is clickable. It acts on the PANE's folder, not
   * the right-clicked file, so a pane on a phone (or a surface with no folder of
   * its own, like the Search dialog) leaves it out and the item shows greyed.
   */
  canOpenTerminalHere?: boolean
  /**
   * Whether "Share…" appears at all (macOS). It hands the selection to the system
   * share sheet, which takes file URLs, so only a pane whose ROWS are ordinary OS
   * paths offers it. Not the same question as `canOpenTerminalHere`: the
   * search-results snapshot has no folder of its own yet lists real files.
   */
  canShare?: boolean
  /**
   * Whether the seven Finder tag colors appear (macOS). A tag is an xattr written through
   * the row's path, so only rows that are real OS paths can hold one; on a phone, an SFTP or
   * WebDAV server, or inside an archive the click would store nothing. Omitting it hides them.
   */
  canTag?: boolean
  /**
   * Whether "Add to favorites" appears on a folder row. A favorite has to point somewhere
   * that's still there next launch and openable from a cold start, so an archive's insides,
   * a `.git`-portal folder, a phone, and a protocol-only server all say no. Omitting it
   * hides the item: Rust's `add_favorite` would refuse anyway, and it refuses silently.
   */
  canFavorite?: boolean
  /**
   * Whether "Copy share link" appears: the row is a FILE on a volume that can mint a
   * link (`canShareLinks`, S3 today). Omitting it hides the item.
   */
  canShareLink?: boolean
}

/**
 * The TEXT the header line at the top of the menu shows about the right-clicked ROW(S),
 * as opposed to what the pane they sit in contributes ({@link PaneContextMenuFacts}).
 *
 * Every field is rendered on this side because every one of them needs locale-aware
 * number formatting, which `crate::intl` deliberately doesn't have. Build them with
 * `$lib/file-explorer/selection/context-menu-target`, never by hand.
 */
export interface ContextMenuTarget {
  /**
   * How many rows the menu acts on, worded for the active language (`3 items`).
   *
   * The backend re-derives the NUMBER from `paths` and uses that to pick the header's
   * shape; this is only the wording for the several-rows shape, which needs the locale's
   * grouping separator and plural form. Omit it below two rows.
   */
  countText?: string
  /**
   * The size the header shows, formatted HERE and passed as text.
   *
   * Omit it when there's no honest size: a folder, a row whose size isn't known yet, a
   * selection with a folder in it (its recursive size may not be settled). ❌ Never a
   * fabricated zero.
   */
  sizeText?: string
}

/**
 * Where a context menu opens, in VIEWPORT CSS pixels — straight out of
 * `getBoundingClientRect()`, no `devicePixelRatio` arithmetic.
 *
 * Only the keyboard path fills one in. A right-click passes nothing and macOS pops the
 * menu at the pointer, which is why every mouse path is untouched by it.
 *
 * The viewport and the native window share an origin here (the webview fills the whole
 * window, title bar included), so no offset is applied. Measurement and the caveat that
 * would change it: `$lib/file-explorer/pane/DETAILS.md` § Keyboard context menu.
 */
export interface MenuAnchor {
  x: number
  y: number
}

/** Every pane fact the backend expects, with an omitted one read as "no". */
function paneFactsForIpc(pane: PaneContextMenuFacts): Required<PaneContextMenuFacts> {
  return {
    restrictDestinationActions: pane.restrictDestinationActions ?? false,
    canShowInFolder: pane.canShowInFolder ?? false,
    listingId: pane.listingId ?? '',
    canOpenTerminalHere: pane.canOpenTerminalHere ?? false,
    canShare: pane.canShare ?? false,
    canTag: pane.canTag ?? false,
    canFavorite: pane.canFavorite ?? false,
    canShareLink: pane.canShareLink ?? false,
  }
}

/**
 * Shows a native context menu for a file.
 * @param path - Absolute path to the right-clicked file (the "primary" file).
 * @param filename - Name of the right-clicked file.
 * @param isDirectory - Whether the entry is a directory.
 * @param paths - All paths the menu's actions should affect. For a right-click on a non-selected
 *                file, pass `[path]`. For a right-click on a file that's part of a multi-selection,
 *                pass the full selection so "Open with" launches all files at once.
 * @param pane - What the surface the click landed in contributes. See {@link PaneContextMenuFacts}.
 * @param target - What the right-clicked rows contribute. See {@link ContextMenuTarget}.
 * @param shortcuts - Every combo bound right now, from `boundShortcuts()` in
 *                    `$lib/shortcuts`. It's what each item's accelerator LABEL is drawn from, so
 *                    the menu tells the truth after a rebind. ❌ Never hand-build it, and ❌ never
 *                    pass display spellings: `boundShortcuts` explains both.
 * @param anchor - Where to open it ({@link MenuAnchor}). Omit for a right-click, so macOS
 *                 uses the pointer.
 * @param sameKind - What the `Selection >` submenu's "Select all of the same kind" row would
 *                   select from this row, from `sameKindTargetFor(entry)`. ❗ Compute it as you
 *                   call: ❌ never read the menu bar's debounced value, which is a frame behind.
 */
export async function showFileContextMenu(
  path: string,
  filename: string,
  isDirectory: boolean,
  paths: string[],
  pane: PaneContextMenuFacts = {},
  target: ContextMenuTarget = {},
  shortcuts: Record<string, string> = {},
  anchor: MenuAnchor | null = null,
  sameKind: SameKindTarget | null = null,
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('show_file_context_menu', {
    path,
    filename,
    isDirectory,
    paths,
    pane: paneFactsForIpc(pane),
    target: {
      countText: target.countText ?? null,
      sizeText: target.sizeText ?? null,
    },
    shortcuts,
    anchor,
    sameKind,
  })
}

/**
 * The Google Drive web URLs for an item, or `null` when the path isn't one we can
 * identify. Backs "Open in Google Drive", "Copy Google Drive link", and "Ask Gemini";
 * a `null` simply means those actions have nothing to act on. One call resolves the
 * Drive item once, and each caller picks the URL it needs — `geminiUrl` is absent for
 * folders, which Gemini can't take as a subject.
 */
export async function googleDriveLinks(path: string): Promise<DriveItemLinks | null> {
  const res = await commands.googleDriveLinks(path)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Mints a share link to the file at `path` and puts it on the clipboard, in Rust.
 * ❗ The link never comes back here: its signature is a credential, and keeping
 * it out of IPC keeps it out of every frontend log. The outcome is all there is.
 */
export async function copyShareLink(
  volumeId: string,
  path: string,
  expiresIn: ShareLinkExpiry,
): Promise<{ ok: true } | { ok: false; error: VolumeError }> {
  const res = await commands.copyShareLink(volumeId, path, expiresIn)
  return res.status === 'ok' ? { ok: true } : { ok: false, error: res.error }
}

/**
 * Make a cloud-managed file available offline (download it). **iCloud Drive only** —
 * it routes through the macOS ubiquity APIs, which reject third-party providers.
 */
export async function cloudMakeAvailableOffline(path: string): Promise<void> {
  const res = await commands.cloudMakeAvailableOffline(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Evict a cloud-managed file's local copy, leaving a placeholder. Counterpart to
 * `cloudMakeAvailableOffline`.
 */
export async function cloudRemoveDownload(path: string): Promise<void> {
  const res = await commands.cloudRemoveDownload(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Shows a native context menu for the breadcrumb path bar.
 *
 * Pass `ejectVolumeId` + `ejectVolumeName` when the breadcrumb represents an
 * ejectable volume — the menu will include an "Eject ({name})" item that emits
 * a `volume-context-action` event on click (subscribe via `onVolumeContextAction`).
 * Pass both or neither; one without the other is treated as no eject target.
 *
 * @param shortcuts - The same live map {@link showFileContextMenu} takes; its "Copy path" item
 *                    reads its accelerator label out of it.
 * @param ejectVolumeId - Volume to eject when the user clicks the eject item.
 * @param ejectVolumeName - Display name for the "Eject ({name})" label.
 */
export async function showBreadcrumbContextMenu(
  shortcuts: Record<string, string>,
  ejectVolumeId?: string,
  ejectVolumeName?: string,
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('show_breadcrumb_context_menu', {
    shortcuts,
    ejectVolumeId: ejectVolumeId ?? null,
    ejectVolumeName: ejectVolumeName ?? null,
  })
}

/**
 * Shows the minimal `..` parent-row context menu (just "Add to favorites").
 * The full file context menu doesn't fit `..`, so this is its own one-item menu.
 * @param parentPath - The directory the `..` row points at; favorited on click.
 * @param anchor - Where to open it ({@link MenuAnchor}). Omit for a right-click, so macOS
 *                 uses the pointer. `..` is a cursor row like any other, so `⌃⏎` on it
 *                 has to land on the row rather than wherever the mouse was left.
 */
export async function showParentRowContextMenu(parentPath: string, anchor: MenuAnchor | null = null): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic over <R: Runtime>, not in typed bindings
  await invoke('show_parent_row_context_menu', { parentPath, anchor })
}

/**
 * Show a file in the system file manager (reveal in parent folder).
 * On macOS, reveals in Finder. On Linux, uses the default file manager.
 * @param path - Absolute path to the file.
 */
export async function showInFinder(path: string): Promise<void> {
  const res = await commands.showInFinder(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Copy text to clipboard.
 * @param text - Text to copy.
 */
export async function copyToClipboard(text: string): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('copy_to_clipboard', { text })
}

/**
 * Open the native Quick Look panel on the given path (macOS only).
 * No-op on volumes without local-fs access (MTP etc.) and on non-macOS.
 * @param path - Absolute path to the file under the cursor.
 * @param volumeId - Volume id of the path. Backend uses this to gate non-local volumes.
 */
export async function quickLookOpen(path: string, volumeId: string): Promise<void> {
  const res = await commands.quickLookOpen(path, volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Retarget an open Quick Look panel to a new path (macOS only). No-op when the panel isn't
 * currently open. Used by the cursor-follow `$effect` in the file pane.
 */
export async function quickLookSetPath(path: string, volumeId: string): Promise<void> {
  const res = await commands.quickLookSetPath(path, volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Close the Quick Look panel (macOS only). No-op when not open.
 * The backend also emits `quick-look-closed` when the panel is dismissed by ✕ or Esc;
 * the frontend listens for that event in `quick-look-state.svelte.ts`.
 */
export async function quickLookClose(): Promise<void> {
  const res = await commands.quickLookClose()
  if (res.status === 'error') throwIpcError(res.error)
}

/** A Get Info ask that never reached Finder, still carrying the backend's typed reason. */
export class GetInfoFailure extends TypedFailure<GetInfoError> {
  constructor(failure: GetInfoError) {
    super(failure, `get info refused: ${failure.type}`)
    this.name = 'GetInfoFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asGetInfoError(error: unknown): GetInfoError | null {
  return error instanceof GetInfoFailure ? error.failure : null
}

/**
 * Opens Finder's Get Info window for a file (macOS only, no-op on other platforms).
 * Throws {@link GetInfoFailure} when the ask couldn't reach Finder, most notably
 * when the user turned off Cmdr's control of Finder in System Settings.
 * @param path - Absolute path to the file.
 */
export async function getInfo(path: string): Promise<void> {
  const res = await commands.getInfo(path)
  if (res.status === 'error') throw new GetInfoFailure(res.error)
}

/** An editor launch that never started, still carrying the backend's typed reason. */
export class OpenInEditorFailure extends TypedFailure<OpenInEditorError> {
  constructor(failure: OpenInEditorError) {
    super(failure, `open in editor refused: ${failure.type}`)
    this.name = 'OpenInEditorFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asOpenInEditorError(error: unknown): OpenInEditorError | null {
  return error instanceof OpenInEditorFailure ? error.failure : null
}

/**
 * Opens a file in the text editor `appChoice` names.
 *
 * Resolves to a REPORT, not a bare success: the chosen app may be gone (the system
 * default opened the file instead), and `openedInName` names the app that got it.
 * Throws {@link OpenInEditorFailure} only when the launch couldn't be attempted.
 * Linux runs `xdg-open` and ignores the last two arguments.
 *
 * @param path - Absolute path to the file.
 * @param appChoice - The stored `behavior.textEditorApp`: `system`, a bundle id, or an `.app` path.
 * @param askAboutOtherEditors - Whether to also answer `otherEditorsInstalled`, for the one-time hint.
 */
export async function openInEditor(
  path: string,
  appChoice: string,
  askAboutOtherEditors: boolean,
): Promise<EditorOpenReport> {
  const res = await commands.openInEditor(path, appChoice, askAboutOtherEditors)
  if (res.status === 'error') throw new OpenInEditorFailure(res.error)
  return res.data
}

/**
 * The text editors macOS lists on this Mac, the system default, and which one
 * `appChoice` names (macOS only).
 *
 * `chosenId` is the choice in the form to store: pass a "Choose an app…" pick's path
 * and store what comes back. It's `null` when that app is gone. Asked fresh on every
 * render; nothing caches it.
 * @param appChoice - The stored choice, or a freshly picked `.app` path.
 */
export async function listTextEditors(appChoice: string): Promise<TimedOut<TextEditorList>> {
  return await commands.listTextEditors(appChoice)
}

/**
 * The terminal apps installed on this Mac, plus which one `appChoice` names.
 *
 * `appChoice` is the stored `behavior.openTerminalHereApp` value: the frontend
 * owns the settings store, so it hands the choice down rather than having Rust
 * read it back. `chosenId` comes back `null` when that app has been uninstalled.
 *
 * Cheap enough to ask on every render (one LaunchServices lookup per known app),
 * so nothing caches it and there's no refresh button.
 * @param appChoice - The stored choice: a bundle id, or an absolute `.app` path.
 */
export async function listTerminalApps(appChoice: string): Promise<TimedOut<TerminalAppList>> {
  return await commands.listTerminalApps(appChoice)
}

/** A launch that never started, still carrying the backend's typed reason. */
export class OpenTerminalFailure extends TypedFailure<OpenTerminalError> {
  constructor(failure: OpenTerminalError) {
    super(failure, `open terminal here refused: ${failure.type}`)
    this.name = 'OpenTerminalFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asOpenTerminalError(error: unknown): OpenTerminalError | null {
  return error instanceof OpenTerminalFailure ? error.failure : null
}

/**
 * Opens `path` in the terminal app `appChoice` names.
 *
 * Resolves to an OUTCOME, not a bare success: the chosen app may have been
 * uninstalled (Terminal opened instead), and the volume may hand out no path a
 * shell can reach (nothing opened). Throws {@link OpenTerminalFailure} only when
 * the launch itself couldn't be attempted.
 *
 * @param path - The folder the pane resolved (the cursor's folder, or its own).
 * @param volumeId - The volume that path came from; the path-less refusal keys on it.
 * @param appChoice - The stored choice: a bundle id, or an absolute `.app` path.
 */
export async function openTerminalHere(
  path: string,
  volumeId: string,
  appChoice: string,
): Promise<OpenTerminalOutcome> {
  const res = await commands.openTerminalHere(path, volumeId, appChoice)
  if (res.status === 'error') throw new OpenTerminalFailure(res.error)
  return res.data
}

/**
 * What to call the app `appChoice` names, or `null` when Cmdr carries no name for
 * it. A table lookup, which is all that's left once the app is uninstalled: the
 * "that app is gone" toast needs the name at exactly the moment there's no bundle
 * to read one from.
 */
export async function terminalAppDisplayName(appChoice: string): Promise<string | null> {
  return await commands.terminalAppDisplayName(appChoice)
}
