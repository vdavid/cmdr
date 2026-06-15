/**
 * Viewer media composable: owns the inline-media (Image / PDF) concern of the
 * viewer window.
 *
 * The viewer renders text through the virtual-scroll line machinery and media
 * inline from the backend's `cmdr-media://` scheme. This composable holds the
 * media session state (`kind` / `mediaToken` / `mediaDimensions`), derives
 * whether the current session is media (`isMedia`) and the URL the `<img>` /
 * `<embed>` loads (`mediaSrc`), remembers the file's natural media kind across a
 * switch to text (`lastMediaKind`), and orchestrates the two-way switch between
 * the rendered media and the raw text view (`viewAsText` / `viewAsMedia`).
 *
 * It follows the same shape as the other `createViewer*` composables: getter /
 * callback deps so it reads and drives the page's reactive state without
 * receiving `$state` directly (which would lose reactivity across the module
 * boundary).
 */

import type { MediaDimensions, ViewerContentKind, ViewerOpenResult } from '$lib/ipc/bindings'
import { isMediaKind, mediaUrl } from './media-view'

interface ViewerMediaDeps {
  /**
   * Re-opens the file as a fresh full text session (`viewerOpenAsText`) and
   * swaps the page to it. The composable owns the media-state teardown around
   * this call; the page owns the actual session open + listener wiring.
   */
  reopenAsText: () => Promise<void>
  /**
   * Re-opens the file via the normal `viewer_open` path, which re-classifies it
   * back to its natural media kind (image / PDF) and swaps the page to it. The
   * reverse of `reopenAsText`. The page wires `setFromOpenResult` so the new
   * media kind / token / dimensions flow back in.
   */
  reopenNatural: () => Promise<void>
}

export function createViewerMedia(deps: ViewerMediaDeps) {
  /**
   * Content kind from the backend. `text` flows through the line / virtual-scroll
   * pipeline; `image` / `pdf` render inline from `mediaToken` via the
   * `cmdr-media://` scheme and leave the text fields empty. Every text-only data
   * path and control on the page guards on `kind === 'text'` (via `isMedia`).
   */
  let kind = $state<ViewerContentKind>('text')
  let mediaToken = $state<string | null>(null)
  let mediaDimensions = $state<MediaDimensions | null>(null)
  /**
   * The file's natural media kind, remembered across a switch to text. A viewer
   * window shows exactly one file for its life, so once we've seen it open as
   * media, the kind stays recoverable even while the user reads it as text. This
   * is what the text view "remembers" to offer switching back to (`viewAsMedia`).
   * Stays `null` for a genuine text file (nothing to switch to).
   */
  let lastMediaKind = $state<ViewerContentKind | null>(null)

  const isMedia = $derived(isMediaKind(kind))
  const mediaSrc = $derived(mediaToken !== null ? mediaUrl(mediaToken) : '')

  /** Absorbs the media fields of a fresh `viewer_open` result. */
  function setFromOpenResult(result: ViewerOpenResult): void {
    kind = result.kind
    mediaToken = result.mediaToken
    mediaDimensions = result.mediaDimensions
    if (isMediaKind(result.kind)) lastMediaKind = result.kind
  }

  /**
   * Resets to the text-session shape. Used up front in the "View as text"
   * override so a re-open failure can't leave a stale image rendered with no
   * live session behind it. PRESERVES `lastMediaKind`: the text view needs it to
   * offer switching back to the natural media kind.
   */
  function reset(): void {
    kind = 'text'
    mediaToken = null
    mediaDimensions = null
  }

  /**
   * "View as text" override for a media file. Resets the media state up front,
   * then asks the page to open a fresh full text session and swap to it. A no-op
   * for a text session (nothing to switch to).
   */
  async function viewAsText(): Promise<void> {
    if (kind === 'text') return
    reset()
    await deps.reopenAsText()
  }

  /**
   * "View as image / PDF" reverse switch, available only while reading a media
   * file as text. Re-opens the file via the normal `viewer_open` path, which
   * re-classifies it back to its natural media kind. A no-op unless we're in text
   * view of a file that was originally media (`lastMediaKind` remembers that).
   */
  async function viewAsMedia(): Promise<void> {
    if (kind !== 'text' || lastMediaKind === null) return
    await deps.reopenNatural()
  }

  return {
    get kind() {
      return kind
    },
    get lastMediaKind() {
      return lastMediaKind
    },
    get mediaDimensions() {
      return mediaDimensions
    },
    get isMedia() {
      return isMedia
    },
    get mediaSrc() {
      return mediaSrc
    },
    setFromOpenResult,
    reset,
    viewAsText,
    viewAsMedia,
  }
}
