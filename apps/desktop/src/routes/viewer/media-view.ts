// Pure helpers for the viewer's media (Image / PDF) rendering branch.
//
// The viewer renders text through the virtual-scroll line machinery and media
// (images, PDFs) inline from the backend's `cmdr-media://` scheme. These
// pure functions hold the bits worth testing in isolation: the media URL
// construction (kept in ONE place because the exact origin form may need a
// one-line tweak after a live check), kind classification, and the
// image fit/zoom math.

import type { MediaDimensions, ViewerContentKind } from '$lib/ipc/bindings'
import { formatInteger } from '$lib/intl/number-format'

/**
 * Builds the URL the `<img>` / `<embed>` loads for a media session.
 *
 * The origin form (`cmdr-media://localhost/<token>`) is the single source of
 * truth for the scheme URL. Keep it here so a live-verified tweak to the
 * authority/host shape is a one-line change.
 */
export function mediaUrl(token: string): string {
  return `cmdr-media://localhost/${encodeURIComponent(token)}`
}

/** Whether a kind renders as inline media (image / PDF) rather than text. */
export function isMediaKind(kind: ViewerContentKind): boolean {
  return kind === 'image' || kind === 'pdf'
}

/** The sentence-case label the picker and status bar show for a kind. */
export function mediaKindLabel(kind: ViewerContentKind): string {
  switch (kind) {
    case 'image':
      return 'Image'
    case 'pdf':
      return 'PDF'
    case 'text':
      return 'Text'
  }
}

/**
 * The picker's reverse-switch label for a media kind: "View as image" / "View as
 * PDF". Lowercases "image" (sentence case) but keeps "PDF" uppercase. `text` has
 * no media render to switch to, so it falls back to a plain "View as text"-shaped
 * string (never shown: callers only use this for a remembered media kind).
 */
export function viewAsMediaLabel(kind: ViewerContentKind): string {
  switch (kind) {
    case 'image':
      return 'View as image'
    case 'pdf':
      return 'View as PDF'
    case 'text':
      return 'View as text'
  }
}

/** Formats pixel dimensions for the status bar (en-US `1,920 × 1,080`, de-DE `1.920 × 1.080`), or null when absent. */
export function formatMediaDimensions(dimensions: MediaDimensions | null): string | null {
  if (dimensions === null) return null
  return `${formatInteger(dimensions.width)} × ${formatInteger(dimensions.height)}`
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
