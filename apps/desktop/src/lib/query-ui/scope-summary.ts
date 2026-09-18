/**
 * Naming the ground a run covered, for the no-results state.
 *
 * A search is ALWAYS scoped: an empty scope box means the focused pane's current folder, never
 * "everywhere" (`$lib/search/search-runners.ts`). The chip says so in the strip, but the
 * no-results state used to list only the query, size, and date criteria, so "nothing in this
 * folder" and "nothing on this drive" read identically. A user reported search as broken
 * because of it (`ERR-FCAXU`: "linear" found nothing in the pane's folder while Finder,
 * searching This Mac, found five files).
 *
 * Pure and consumer-agnostic on purpose: Selection has no scope row, and `query-ui` must not
 * reach into `$lib/search`.
 */

/** Everything the two helpers need, as the dialog already holds it. */
export interface ScopeSummaryInput {
  /** The scope box's contents. Empty means "the default applies". */
  scope: string
  /** Where an empty box actually searches. */
  defaultScopePath: string
  /** What the chip calls that default ("Current folder" / "This volume"). */
  defaultScopeLabel: string
}

/**
 * The path a run is actually confined to: what the user typed, else the resolved default.
 * A typed scope may be a comma-separated list, so treat this as a summary, not a single path.
 */
export function effectiveScopePath({ scope, defaultScopePath }: ScopeSummaryInput): string {
  return scope.trim() || defaultScopePath
}

/**
 * How to NAME that ground in a sentence ("Searched in: Downloads").
 *
 * A typed scope shows verbatim: the user wrote it, so it's already the words they think in.
 * A defaulted one shows the folder's own name, which is what makes the constraint land
 * ("Downloads" tells you what went wrong; "Current folder" only restates the setting). We fall
 * back to the chip's label when there's no name to show: a volume root has no last segment, and
 * a pane sitting in the home folder reports the bare `~`, which would read as punctuation.
 */
export function scopeSummaryFor(input: ScopeSummaryInput): string {
  const typed = input.scope.trim()
  if (typed) return typed
  return folderNameOf(input.defaultScopePath) || input.defaultScopeLabel
}

/**
 * Last path segment, or `''` when there isn't a usable one. Tolerant by design: this runs on
 * whatever the pane reported, which may be `/`, `~`, a trailing-slash mount root, or a
 * `scheme://host/share` remote path.
 */
function folderNameOf(path: string): string {
  const segments = path.split('/').filter((s) => s.length > 0)
  const last = segments.at(-1) ?? ''
  if (last === '~') return ''
  // A scheme-rooted path splits its scheme into the first segment: `smb://nas.local` becomes
  // ["smb:", "nas.local"]. At that depth the tail is the HOST, not a folder, so there's no name
  // to show. Deeper (`smb://nas.local/share/photos`) the tail is an ordinary folder again.
  if (segments.length <= 2 && segments[0]?.endsWith(':')) return ''
  return last
}
