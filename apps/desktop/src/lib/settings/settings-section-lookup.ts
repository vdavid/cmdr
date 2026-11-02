/**
 * A section's path from one name, for a caller that holds only a word: the `open-settings` event,
 * which an AI tool's `section` reaches with whatever the model chose. The page renders nothing for a
 * path that isn't a section, so an unknown name has to stop here.
 */

import { buildSectionTree } from './settings-registry'
import type { SettingsSection } from './types'

/** The sidebar pages with dedicated UI, which the registry's tree doesn't hold. */
export const SPECIAL_SETTINGS_SECTIONS = {
  'Keyboard shortcuts': { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'] },
  License: { name: 'License', path: ['License'] },
} as const satisfies Record<string, { name: string; path: string[] }>

function everySection(): { name: string; path: readonly string[] }[] {
  const walk = (nodes: SettingsSection[]): { name: string; path: readonly string[] }[] =>
    nodes.flatMap((node) => [{ name: node.name, path: node.path }, ...walk(node.subsections)])
  return [...walk(buildSectionTree()), ...Object.values(SPECIAL_SETTINGS_SECTIONS)]
}

/**
 * The path of the section with this name, ignoring case, or of the one section whose name holds it.
 * `undefined` for no section, and for several: guessing would open the wrong page and call it right.
 */
export function resolveSettingsSection(name: string): string[] | undefined {
  const wanted = name.trim().toLowerCase()
  if (wanted === '') return undefined
  const sections = everySection()
  const exact = sections.find((section) => section.name.toLowerCase() === wanted)
  if (exact) return [...exact.path]
  const partial = sections.filter((section) => section.name.toLowerCase().includes(wanted))
  return partial.length === 1 ? [...partial[0].path] : undefined
}
