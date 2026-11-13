/**
 * The brief's per-key note for a per-OS `{platform, select, …}` value: which OS reads each of
 * the key's own branches, where each OS's words live in the reference pile, and that the branch
 * macOS reads keeps its translation. The rule it applies is
 * `docs/i18n/translator-instructions.md` (Per-OS values); this only maps one key onto it.
 */
import { platformBranches } from './i18n-platform-branches.ts'
import type { RenderedPlatform } from './i18n-platform-branches.ts'

/** Each OS's name, and the pile folders its own words come from. */
const OS_SOURCES: Readonly<Record<RenderedPlatform, { name: string; pile: string }>> = {
  macos: { name: 'macOS', pile: '`macOS/`' },
  windows: { name: 'Windows', pile: '`microsoft-terminology/`, `microsoft-style-guides/`' },
  linux: { name: 'Linux', pile: '`gnome-nautilus/`, `kde-dolphin/`, `xfce-thunar/`' },
}

/**
 * The note, or `undefined` for a plain value. A block without a `macos` (or `windows`) branch
 * shows `other` on that OS too, so `other`'s readers depend on which branches the key has.
 * @param compact the short form for every per-OS key after a brief's first: which OS reads each
 *   branch and which one macOS reads, without the pile folders and the frame rule the first key's
 *   note already gave (88 full notes were 8% of a 110k-token brief)
 */
export function perOsNote(value: string, compact = false): string | undefined {
  const branches = platformBranches(value)
  if (branches[0].name === null) return undefined
  const names = new Set(branches.map(({ name }) => name))
  const parts = branches.map(({ name }) => {
    const oses: RenderedPlatform[] =
      name === 'other'
        ? [
            ...(names.has('macos') ? [] : ['macos' as const]),
            ...(names.has('windows') ? [] : ['windows' as const]),
            'linux',
          ]
        : [name === 'macos' ? 'macos' : 'windows']
    const who = oses.map((os) => OS_SOURCES[os].name).join(' and ')
    const piles = oses.map((os) => OS_SOURCES[os].pile).join('; ')
    if (compact) return `\`${String(name)}\` is ${who}${oses.includes('macos') ? ' (macOS reads it: keep)' : ''}`
    const keep = oses.includes('macos') ? ', and macOS reads it, so a translation already there stays' : ''
    return `\`${String(name)}\` is ${who} (pile: ${piles})${keep}`
  })
  if (compact) return `Per-OS value: ${parts.join('; ')}.`
  return `Per-OS value (see the Per-OS values instruction): ${parts.join('; ')}. Keep the \`{platform, select, …}\` frame and the branch names exactly as they are.`
}
