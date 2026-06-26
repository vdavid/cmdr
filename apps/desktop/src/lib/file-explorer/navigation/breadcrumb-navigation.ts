/**
 * Turns the breadcrumb's display segments into clickable navigation targets.
 *
 * The breadcrumb renders a DISPLAY path (volume prefix stripped, home collapsed
 * to `~`); clicking a segment must navigate to the REAL ancestor path. Both
 * transforms are prefix-only, so we rebuild each ancestor by joining the visible
 * segment texts onto the right base:
 *
 *   - `~`-rooted: the base is the home dir (`~` → `userHomePath`).
 *   - non-root volume: the base is the volume path (display is volume-relative).
 *   - MTP: rebuild the `mtp://device/storage/...` URL from the parsed current path.
 *   - root volume, absolute: no base; the leading `/` comes from the root marker.
 *
 * The last segment (the current folder) and the empty leading root marker are
 * never clickable, and a search-results pane (whose "path" is a query label, not
 * a real path) is never clickable.
 */

import { isMtpVolumeId, parseMtpPath, constructMtpPath } from '$lib/mtp'
import type { PathSegment } from './path-segments'

export interface BreadcrumbNavContext {
  /** The pane's volume id (drives the MTP branch). */
  volumeId: string
  /** The volume's root path (`/` for the root volume, `mtp://…`/`smb://…` for virtual). */
  volumePath: string
  /** The pane's current real path (canonical-ish; what the display path derives from). */
  currentPath: string
  /** The user's home dir, no trailing slash; `''` until it resolves on mount. */
  userHomePath: string
  /** Whether this pane shows a search-results snapshot (label, not a path). */
  isSearchResults: boolean
}

export interface ClickableBreadcrumbSegment extends PathSegment {
  /** The real navigable target for clicking this segment, or `null` if it isn't clickable. */
  target: string | null
  /** The friendly path shown in the tooltip (the visible display prefix up to this segment). */
  displayPath: string
}

/** Joins the visible texts of `segments[1..=index]` into an inner path (no leading slash). */
function innerPath(segments: PathSegment[], index: number): string {
  return segments
    .slice(1, index + 1)
    .map((s) => s.text)
    .join('/')
}

/** The real navigation target for clicking segment `index`, or `null` if not navigable. */
function targetFor(segments: PathSegment[], index: number, ctx: BreadcrumbNavContext): string | null {
  if (ctx.isSearchResults) return null

  if (isMtpVolumeId(ctx.volumeId)) {
    const parsed = parseMtpPath(ctx.currentPath)
    if (!parsed) return null
    return constructMtpPath(parsed.deviceId, parsed.storageId, innerPath(segments, index))
  }

  const first = segments[0]?.text
  let base: string
  if (first === '~') {
    // Can't expand `~` until the home dir resolves.
    if (!ctx.userHomePath) return null
    base = ctx.userHomePath
  } else if (ctx.volumePath && ctx.volumePath !== '/') {
    base = ctx.volumePath
  } else {
    base = ''
  }

  const tail = innerPath(segments, index)
  return tail ? `${base}/${tail}` : base || '/'
}

/**
 * Enriches breadcrumb display segments with a navigation `target` (null when not
 * clickable) and a friendly `displayPath` for the tooltip.
 */
export function enrichBreadcrumbSegments(
  segments: PathSegment[],
  ctx: BreadcrumbNavContext,
): ClickableBreadcrumbSegment[] {
  const lastIndex = segments.length - 1
  return segments.map((seg, i) => {
    // The current folder (last) and the empty root marker have nothing to navigate to.
    const clickable = !ctx.isSearchResults && i < lastIndex && seg.text !== ''
    return {
      ...seg,
      target: clickable ? targetFor(segments, i, ctx) : null,
      displayPath: segments
        .slice(0, i + 1)
        .map((s) => s.text)
        .join('/'),
    }
  })
}
