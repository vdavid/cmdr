/**
 * Viewer media composable: owns the inline-media (Image / PDF) concern of the
 * viewer window.
 *
 * The viewer renders text through the virtual-scroll line machinery and media
 * inline from the backend's `cmdr-media://` scheme. This composable holds the
 * media session state (`kind` / `mediaToken` / `mediaDimensions`), derives
 * whether the current session is media (`isMedia`) and the URL the `<img>` /
 * `<embed>` loads (`mediaSrc`), and orchestrates the "View as text" override.
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

  const isMedia = $derived(isMediaKind(kind))
  const mediaSrc = $derived(mediaToken !== null ? mediaUrl(mediaToken) : '')

  /** Absorbs the media fields of a fresh `viewer_open` result. */
  function setFromOpenResult(result: ViewerOpenResult): void {
    kind = result.kind
    mediaToken = result.mediaToken
    mediaDimensions = result.mediaDimensions
  }

  /**
   * Resets to the text-session shape. Used up front in the "View as text"
   * override so a re-open failure can't leave a stale image rendered with no
   * live session behind it.
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

  return {
    get kind() {
      return kind
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
  }
}
