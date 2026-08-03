/**
 * Turns the bare commit hashes in `CHANGELOG.md` into GitHub links.
 *
 * The changelog stores each entry's commit references as a bare trailing group,
 * `- Some change (b626d7a4, 2d41cc14)`, because the linked form cost about 40% of
 * the file in URL boilerplate and made it painful to read (and to feed to an
 * agent). Every renderer linkifies on the way out instead: this one for the
 * website, and the app's What's new parser, which strips the group entirely.
 *
 * The recognition rule is shared across all three implementations (see
 * `scripts/check/checks/DETAILS.md` § CHANGELOG commit refs): a group is the
 * trailing parenthetical of a bullet entry, and every comma-separated item in it
 * is 6–40 lowercase hex chars. Anchoring to the end is what keeps prose safe, so
 * an aside like `(~40x speed-up!)` or a mid-sentence `(added)` is never touched.
 */

const REPO_URL = 'https://github.com/vdavid/cmdr'

/** A group of hashes closing a logical entry. Group 1 is the comma-separated list. */
const trailingHashGroupPattern = /\(([0-9a-f]{6,40}(?:,\s*[0-9a-f]{6,40})*)\)$/

const bulletMarkers = ['- ', '* ', '+ ']

/** True when the line opens a bullet entry: a list marker at column zero. */
function startsEntry(line: string): boolean {
  return bulletMarkers.some((marker) => line.startsWith(marker))
}

/** True when the line is a wrapped continuation of the entry above it. */
function isContinuation(line: string): boolean {
  return line.trim() !== '' && /^[ \t]/.test(line)
}

/**
 * Rewrites one entry's hash group in place across the source lines it spans, so
 * the markdown keeps its original wrapping.
 */
function linkifyEntry(lines: string[], entryLineNumbers: number[]): void {
  const joined = entryLineNumbers
    .map((n) => lines[n].trim())
    .join(' ')
    .trim()
  const match = trailingHashGroupPattern.exec(joined)
  if (!match) return

  const hashes = match[1].split(',').map((hash) => hash.trim())
  // The group opens with `(` immediately followed by its first hash, which pins
  // down the source line the group starts on. Everything from there to the end of
  // the entry is group text, so replacing hashes there can't touch prose.
  const groupOpening = `(${hashes[0]}`
  const startIndex = entryLineNumbers.findIndex((n) => lines[n].includes(groupOpening))
  if (startIndex < 0) return

  const hashPattern = new RegExp(`(?<![0-9a-f])(${hashes.join('|')})(?![0-9a-f])`, 'g')
  for (const lineNumber of entryLineNumbers.slice(startIndex)) {
    lines[lineNumber] = lines[lineNumber].replace(hashPattern, (hash) => `[${hash}](${REPO_URL}/commit/${hash})`)
  }
}

/** Linkifies every trailing commit-hash group in a changelog markdown document. */
export function linkifyCommitHashes(markdown: string): string {
  const lines = markdown.split('\n')
  let entry: number[] = []

  const flush = (): void => {
    if (entry.length > 0) linkifyEntry(lines, entry)
    entry = []
  }

  for (const [lineNumber, line] of lines.entries()) {
    if (startsEntry(line)) {
      flush()
      entry = [lineNumber]
    } else if (entry.length > 0 && isContinuation(line)) {
      entry.push(lineNumber)
    } else {
      flush()
    }
  }
  flush()

  return lines.join('\n')
}
