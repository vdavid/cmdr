/**
 * Splits a display path into renderable segments and flags any that live
 * inside a `.git/...` portal so the breadcrumb can color them with the
 * git-portal token.
 *
 * The first segment after a `.git` is the start of the portal; every
 * subsequent segment stays inside it. The `.git` segment itself is also
 * colored: that's where the portal opens.
 */
export interface PathSegment {
  /** Visible text. */
  text: string
  /** Whether to render this segment using `--color-git-portal-text`. */
  gitPortal: boolean
}

/**
 * Splits `displayPath` on `/` and walks the segments; the first `.git`
 * encountered (and everything after it) is flagged as git-portal.
 *
 * Empty leading segments (from a leading `/`) are preserved as the root
 * marker so the joined output round-trips. Empty mid-path segments
 * (from doubled slashes) are dropped. They're never legitimate.
 */
export function splitPathSegments(displayPath: string): PathSegment[] {
  if (displayPath.length === 0) return []
  const raw = displayPath.split('/')
  const segments: PathSegment[] = []
  let inGit = false
  for (let i = 0; i < raw.length; i++) {
    const text = raw[i]
    if (text.length === 0) {
      // Keep the leading slash marker; drop interior empties.
      if (i === 0) segments.push({ text: '', gitPortal: false })
      continue
    }
    if (text === '.git') {
      inGit = true
    }
    segments.push({ text, gitPortal: inGit })
  }
  return segments
}
