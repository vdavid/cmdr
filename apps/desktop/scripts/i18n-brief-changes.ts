/**
 * How a batch key's value changed and where a target stands on it, as the brief's keys section
 * prints them: the English word diff (`wordDiff`, `changeLines`) and a target's current value
 * (`currentValueText`). Both say a per-OS block's repeated text once, since a brief that prints
 * every branch three times is what a translator pays for on every turn.
 */
import { platformBranches, visibleJson } from './i18n-catalog-lib.ts'

/**
 * A word-level diff, `[-removed-]` and `{+added+}` around the words that changed
 * (split on whitespace, so a placeholder or a punctuated word is one word).
 */
export function wordDiff(before: string, after: string): string {
  const a = before.split(/\s+/).filter(Boolean)
  const b = after.split(/\s+/).filter(Boolean)
  // Longest common subsequence, table from the ends.
  const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1])
    }
  }
  const out: string[] = []
  let removed: string[] = []
  let added: string[] = []
  const flush = () => {
    if (removed.length > 0) out.push(`[-${removed.join(' ')}-]`)
    if (added.length > 0) out.push(`{+${added.join(' ')}+}`)
    removed = []
    added = []
  }
  let i = 0
  let j = 0
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      flush()
      out.push(a[i])
      i++
      j++
    } else if (j < b.length && (i === a.length || lcs[i][j + 1] >= lcs[i + 1][j])) added.push(b[j++])
    else removed.push(a[i++])
  }
  flush()
  return out.join(' ')
}

/**
 * One line per distinct older English the targets translated from: the word diff to today's. An English that only
 * became a per-OS block, its old text living on word for word as a branch, gets a sentence saying so: its diff would
 * repeat the whole value (10% of each brief in the first per-OS batches).
 */
export function changeLines(previous: Record<string, string> | undefined, current: string): string[] {
  const byEnglish = new Map<string, string[]>()
  for (const [tag, english] of Object.entries(previous ?? {}))
    byEnglish.set(english, [...(byEnglish.get(english) ?? []), tag])
  const branches = platformBranches(current)
  return [...byEnglish].map(([english, tags]) => {
    const kept = branches.filter(({ name, text }) => name !== null && text === english).map(({ name }) => name)
    const change =
      kept.length > 0
        ? `it became a per-OS block, and ${kept.map((name) => `\`${String(name)}\``).join(' and ')} ${kept.length > 1 ? 'are' : 'is'} the old text, word for word`
        : wordDiff(english, current)
    return `  - English changed since ${tags.join(', ')} translated it: ${change}`
  })
}

/**
 * A target's current value as the keys section shows it. A per-OS block whose branches line up with the English
 * ones shows only the branches that differ from English, and names the ones still in English: printing those in
 * full would repeat the English right above (a tenth of a per-OS batch's brief).
 */
export function currentValueText(english: string, current: string): string {
  const source = platformBranches(english)
  const target = platformBranches(current)
  const sameShape =
    source[0].name !== null &&
    source.length === target.length &&
    source.every(({ name }, index) => target[index].name === name)
  if (!sameShape) return visibleJson(current)
  const pending = target.filter(({ text }, index) => text === source[index].text)
  if (pending.length === 0) return visibleJson(current)
  const done = target.filter(({ text }, index) => text !== source[index].text)
  const names = (branches: typeof target) => branches.map(({ name }) => `\`${String(name)}\``).join(' and ')
  const still = `${names(pending)} still in English (as above)`
  if (done.length === 0) return still
  return `${done.map(({ name, text }) => `\`${String(name)}\` ${visibleJson(text)}`).join('; ')}; ${still}`
}
