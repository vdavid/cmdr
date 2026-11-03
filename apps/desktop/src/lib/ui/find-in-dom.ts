/**
 * Find-in-page over rendered DOM text: what a find bar searches when the content is a
 * live, component-owned tree (the Ask Cmdr thread) rather than a model the caller owns
 * (the viewer's rows).
 *
 * Two halves, both read-only toward the DOM:
 *
 * - `findTextRanges` walks the TEXT NODES of the regions a container marks with
 *   `data-find-region`, so it matches exactly what the person sees: rendered Markdown
 *   without its asterisks, a tool line's own words. It never builds HTML from the text,
 *   so a match can't become markup.
 * - `paintFindMatches` marks the hits through the CSS Custom Highlight API, which styles
 *   ranges without touching a node. Wrapping hits in `<mark>` would split text nodes a
 *   component (Svelte) still holds and updates, garbling the next update. Where the API
 *   is missing (WebKit before Safari 17.2, so macOS 10.15 and 11), the current match's
 *   region carries `data-find-current` instead, which the owner outlines.
 *
 * Ranges go stale when the content re-renders (a streaming answer replaces its nodes), so
 * the owner re-runs `findTextRanges` on every content change.
 */

/** The attribute that opts an element's text into find. */
export const FIND_REGION_ATTRIBUTE = 'data-find-region'

/** The attribute the no-Highlight-API fallback puts on the current match's region. */
export const FIND_CURRENT_ATTRIBUTE = 'data-find-current'

/** `::highlight()` names; global per document, so they carry the app's prefix. */
export const FIND_MATCH_HIGHLIGHT = 'cmdr-find-match'
export const FIND_CURRENT_HIGHLIGHT = 'cmdr-find-current'

/**
 * Case-folds `text` one code point at a time, keeping its length: a fold that changes the
 * length (`İ` lowers to two units) keeps the original unit, so an index into the folded
 * string is an index into the original. Folding a whole string at once would shift every
 * later offset by one after such a character.
 */
export function foldCasePreservingLength(text: string): string {
  let out = ''
  for (const unit of text) {
    const lowered = unit.toLowerCase()
    out += lowered.length === unit.length ? lowered : unit
  }
  return out
}

/** Every start offset of `needle` in `haystack`, without overlaps (`aa` in `aaaa` is two). */
export function matchOffsets(haystack: string, needle: string): number[] {
  if (needle === '') return []
  const offsets: number[] = []
  let from = 0
  for (;;) {
    const at = haystack.indexOf(needle, from)
    if (at === -1) return offsets
    offsets.push(at)
    from = at + needle.length
  }
}

/** A region's text nodes, and where each starts in the region's joined text. */
interface RegionText {
  nodes: Text[]
  starts: number[]
  text: string
}

function isHiddenFromFind(node: Node, region: Element): boolean {
  for (let el = node.parentElement; el && el !== region; el = el.parentElement) {
    if (el.getAttribute('aria-hidden') === 'true') return true
  }
  return false
}

function regionText(region: Element): RegionText {
  const walker = region.ownerDocument.createTreeWalker(region, NodeFilter.SHOW_TEXT)
  const nodes: Text[] = []
  const starts: number[] = []
  let text = ''
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (isHiddenFromFind(node, region)) continue
    const value = node.nodeValue ?? ''
    if (value === '') continue
    nodes.push(node as Text)
    starts.push(text.length)
    text += value
  }
  return { nodes, starts, text }
}

/** The text node holding joined offset `offset`, and the offset inside it. `end` picks the left node at a seam. */
function locate(region: RegionText, offset: number, end: boolean): { node: Text; offset: number } {
  let index = region.nodes.length - 1
  for (let i = 0; i < region.nodes.length; i++) {
    const nodeEnd = region.starts[i] + (region.nodes[i].nodeValue ?? '').length
    if (end ? offset <= nodeEnd : offset < nodeEnd) {
      index = i
      break
    }
  }
  return { node: region.nodes[index], offset: offset - region.starts[index] }
}

/**
 * Every case-insensitive hit of `query` in `container`'s find regions, in document order.
 * A hit can span text nodes inside one region (`**bold** text` renders as two), never two
 * regions: a user bubble and the answer below it aren't one sentence. Regions nested in a
 * region count once, as part of the outer one.
 */
export function findTextRanges(container: Element, query: string): Range[] {
  const needle = foldCasePreservingLength(query)
  if (needle.trim() === '') return []
  const ranges: Range[] = []
  for (const region of container.querySelectorAll(`[${FIND_REGION_ATTRIBUTE}]`)) {
    if (region.parentElement?.closest(`[${FIND_REGION_ATTRIBUTE}]`)) continue
    const joined = regionText(region)
    for (const at of matchOffsets(foldCasePreservingLength(joined.text), needle)) {
      const start = locate(joined, at, false)
      const end = locate(joined, at + needle.length, true)
      const range = region.ownerDocument.createRange()
      range.setStart(start.node, start.offset)
      range.setEnd(end.node, end.offset)
      ranges.push(range)
    }
  }
  return ranges
}

/** The index of the first range that starts at or below `top` (a viewport y), else the first. */
export function firstRangeFrom(ranges: readonly Range[], top: number): number {
  if (ranges.length === 0) return -1
  const index = ranges.findIndex((range) => range.getBoundingClientRect().top >= top)
  return index === -1 ? 0 : index
}

interface HighlightRegistry {
  set: (name: string, highlight: unknown) => void
  delete: (name: string) => void
}

type HighlightConstructor = new (...ranges: Range[]) => unknown

function highlightApi(): { registry: HighlightRegistry; Highlight: HighlightConstructor } | null {
  const registry = (globalThis.CSS as { highlights?: HighlightRegistry } | undefined)?.highlights
  const Highlight = (globalThis as { Highlight?: HighlightConstructor }).Highlight
  if (!registry || !Highlight) return null
  return { registry, Highlight }
}

/** Whether the CSS Custom Highlight API can paint matches here. */
export function canPaintFindHighlights(): boolean {
  return highlightApi() !== null
}

function regionOf(range: Range): Element | null {
  const node = range.startContainer
  const el = node instanceof Element ? node : node.parentElement
  return el?.closest(`[${FIND_REGION_ATTRIBUTE}]`) ?? null
}

/**
 * Paints `ranges`, with `current` (an index, or -1) marked apart from the rest. `container`
 * scopes the fallback's attribute cleanup. Safe to call on every change; it replaces what
 * it painted before.
 */
export function paintFindMatches(container: Element, ranges: readonly Range[], current: number): void {
  for (const el of container.querySelectorAll(`[${FIND_CURRENT_ATTRIBUTE}]`)) el.removeAttribute(FIND_CURRENT_ATTRIBUTE)
  const api = highlightApi()
  if (!api) {
    const range = ranges.at(current)
    if (current >= 0 && range) regionOf(range)?.setAttribute(FIND_CURRENT_ATTRIBUTE, '')
    return
  }
  const others = ranges.filter((_, index) => index !== current)
  api.registry.set(FIND_MATCH_HIGHLIGHT, new api.Highlight(...others))
  const currentRange = ranges.at(current)
  if (current >= 0 && currentRange) api.registry.set(FIND_CURRENT_HIGHLIGHT, new api.Highlight(currentRange))
  else api.registry.delete(FIND_CURRENT_HIGHLIGHT)
}

/** Removes everything `paintFindMatches` painted. */
export function clearFindMatches(container: Element | null): void {
  if (container) {
    for (const el of container.querySelectorAll(`[${FIND_CURRENT_ATTRIBUTE}]`))
      el.removeAttribute(FIND_CURRENT_ATTRIBUTE)
  }
  const api = highlightApi()
  api?.registry.delete(FIND_MATCH_HIGHLIGHT)
  api?.registry.delete(FIND_CURRENT_HIGHLIGHT)
}

/**
 * Scrolls `scroller` so `range` is in view, centring it only when it isn't already fully
 * visible (so stepping through matches on one screen doesn't make the thread jump).
 * `prefers-reduced-motion` turns the smooth scroll into a jump.
 */
export function scrollRangeIntoView(scroller: Element, range: Range): void {
  const box = range.getBoundingClientRect()
  const view = scroller.getBoundingClientRect()
  if (box.top >= view.top && box.bottom <= view.bottom) return
  const delta = box.top - view.top - (view.height - box.height) / 2
  const reduce =
    typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  scroller.scrollBy({ top: delta, behavior: reduce ? 'auto' : 'smooth' })
}
