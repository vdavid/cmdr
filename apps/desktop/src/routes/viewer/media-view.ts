// Pure helpers for the viewer's media (Image / PDF / Markdown) rendering branch.
//
// The viewer renders text through the virtual-scroll line machinery and media
// (images, PDFs, a Markdown document's pictures) inline from the backend's
// `cmdr-media://` scheme. These
// pure functions hold the bits worth testing in isolation: the media URL
// construction (kept in ONE place because the exact origin form may need a
// one-line tweak after a live check), kind classification, and the
// image fit/zoom math.

import { convertFileSrc } from '@tauri-apps/api/core'

import type { MediaDimensions, ViewerContentKind } from '#lib/ipc/bindings.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { getMessage, tString } from '#lib/intl/messages.svelte.js'

/**
 * Builds the URL the `<img>` / `<embed>` loads for a media session.
 *
 * The one place a media URL is built: Tauri's `convertFileSrc` gives the
 * webview's form of the `cmdr-media` scheme (`cmdr-media://localhost/<token>` on
 * macOS), and a host with no scheme answers its own (the browser demo: a `blob:`
 * URL over the token's bytes).
 */
export function mediaUrl(token: string): string {
  return convertFileSrc(token, 'cmdr-media')
}

/** Whether a kind has a rendered view (image / PDF / formatted Markdown) rather than only text. */
export function isMediaKind(kind: ViewerContentKind): boolean {
  return kind === 'image' || kind === 'pdf' || kind === 'markdown'
}

/** The dedicated viewer available for the current or previously detected media session. */
export function availableMediaKind(
  kind: ViewerContentKind,
  lastMediaKind: ViewerContentKind | null,
): ViewerContentKind | null {
  if (isMediaKind(kind)) return kind
  return lastMediaKind !== null && isMediaKind(lastMediaKind) ? lastMediaKind : null
}

/** The sentence-case label the picker and status bar show for a kind. */
export function mediaKindLabel(kind: ViewerContentKind): string {
  switch (kind) {
    case 'image':
      return getMessage('viewer.kind.image')
    case 'pdf':
      return getMessage('viewer.kind.pdf')
    case 'markdown':
      return getMessage('viewer.kind.markdown')
    case 'text':
      return getMessage('viewer.kind.text')
  }
}

/**
 * The picker's reverse-switch label for a media kind: "View as image" / "View as
 * PDF". `text` has no media render to switch to, so it falls back to a plain
 * "View as text"-shaped string (never shown: callers only use this for a
 * remembered media kind).
 */
export function viewAsMediaLabel(kind: ViewerContentKind): string {
  switch (kind) {
    case 'image':
      return getMessage('viewer.toolbar.viewMode.viewAsImage')
    case 'pdf':
      return getMessage('viewer.toolbar.viewMode.viewAsPdf')
    case 'markdown':
      return getMessage('viewer.toolbar.viewMode.viewFormatted')
    case 'text':
      return getMessage('viewer.toolbar.viewMode.viewAsText')
  }
}

/** Formats pixel dimensions for the status bar (en-US `1,920 × 1,080`, de-DE `1.920 × 1.080`), or null when absent. */
export function formatMediaDimensions(dimensions: MediaDimensions | null): string | null {
  if (dimensions === null) return null
  return tString('viewer.media.dimensions', {
    width: formatInteger(dimensions.width),
    height: formatInteger(dimensions.height),
  })
}

export const MEDIA_MIN_ZOOM = 0.1
export const MEDIA_MAX_ZOOM = 20

/** Clamps a zoom factor to the allowed range. */
export function clampZoom(zoom: number): number {
  return Math.min(MEDIA_MAX_ZOOM, Math.max(MEDIA_MIN_ZOOM, zoom))
}

/** The image's display mode: `fit` scales to the window; `actual` honors an explicit zoom. */
export type ImageViewMode = 'fit' | 'actual'

/**
 * Decides the next state when the user clicks the image. The interaction toggles
 * between fit-to-window and 100%: from `fit` go to 100%, from any explicit zoom (a
 * scroll-zoomed image included) go back to fit. `zoom: null` means "let fit
 * recompute".
 */
export function nextClickZoom(mode: ImageViewMode): { mode: ImageViewMode; zoom: number | null } {
  if (mode === 'fit') return { mode: 'actual', zoom: 1 }
  return { mode: 'fit', zoom: null }
}
