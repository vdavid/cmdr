/**
 * Reading the two facts the marketing capture needs out of `cmdr://state`.
 *
 * ❗ That resource is YAML-shaped TEXT, not JSON — `JSON.parse` on it throws. Parsing
 * lives here as pure functions so the shapes are pinned by tests rather than by a
 * hopeful regex inside the spec.
 */

export interface PaneTab {
  id: string
  pinned: boolean
}

/**
 * The tabs of one pane, in order.
 *
 * Scoped to the pane's own block on purpose: the two panes' tab lines are identical in
 * shape, so a parser that scans the whole document unpins the other pane's tab and
 * leaves the shot with a layout nobody asked for.
 */
export function parsePaneTabs(state: string, pane: 'left' | 'right'): PaneTab[] {
  const lines = state.split('\n')
  const start = lines.findIndex((line) => line === `${pane}:`)
  if (start < 0) return []
  const tabs: PaneTab[] = []
  let inTabs = false
  for (const line of lines.slice(start + 1)) {
    // A new top-level key ends this pane's block.
    if (/^\S/.test(line)) break
    if (line.trim() === 'tabs:') {
      inTabs = true
      continue
    }
    if (!inTabs) continue
    const entry = /^\s+- i:\d+ id:(\S+)(.*)$/.exec(line)
    if (entry === null) {
      inTabs = false
      continue
    }
    tabs.push({ id: entry[1], pinned: entry[2].includes('[pinned]') })
  }
  return tabs
}

/**
 * A pane's view mode, or `null` when the state doesn't say.
 *
 * ❗ Scoped to the pane's own block for the same reason `parsePaneTabs` is: both panes
 * print an identical `view:` line, and a document-wide scan reads the left pane's mode
 * while claiming it's the right pane's.
 *
 * Callers use it to SKIP a `set_view_mode` that wouldn't change anything: the MCP tool
 * acks on the pane generation advancing, so a call that sets brief on an already-brief
 * pane never acks and times out.
 */
export function parsePaneView(state: string, pane: 'left' | 'right'): string | null {
  const lines = state.split('\n')
  const start = lines.findIndex((line) => line === `${pane}:`)
  if (start < 0) return null
  for (const line of lines.slice(start + 1)) {
    // A new top-level key ends this pane's block.
    if (/^\S/.test(line)) break
    const match = /^\s+view:\s*(\S+)\s*$/.exec(line)
    if (match !== null) return match[1]
  }
  return null
}

/**
 * Whether every volume's index has stopped moving AND at least one is actually indexed.
 *
 * Both halves matter. A `scanning` volume paints an hourglass into every size cell,
 * which is what a whole round of unusable masters looks like; and a state where nothing
 * is indexed at all would otherwise pass this trivially, photographing the same
 * hourglasses for the opposite reason.
 */
export function indexIsSettled(state: string): boolean {
  const statuses = [...state.matchAll(/^\s*indexStatus:\s*(\S+)\s*$/gm)].map((match) => match[1])
  if (statuses.some((status) => status === 'scanning')) return false
  return statuses.includes('fresh')
}
