/**
 * Maps a section's English structural NAME (the `section: string[]` identity in
 * the registry, used for routing/search/tree) to its rendered, translated TITLE.
 *
 * Section names stay English in the registry on purpose — they're a stable key,
 * not a render path. The sidebar and the section-summary cards render titles, so
 * they resolve through here. One source for the name→key map keeps the sidebar
 * and summary in step; an unmapped name falls back to the name itself (so a new
 * section without a key still renders, just untranslated, until catalogued).
 */

import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * Section English name → its title catalog key. `Partial` so an unmapped name
 * reads as `undefined` (the lookup below falls back to the name itself) rather
 * than being typed as an always-present `MessageKey`.
 */
const SECTION_TITLE_KEY: Partial<Record<string, MessageKey>> = {
  Appearance: 'settings.section.appearance',
  'Colors and formats': 'settings.section.colorsAndFormats',
  'Zoom and density': 'settings.section.zoomAndDensity',
  'File and folder sizes': 'settings.section.fileAndFolderSizes',
  Listing: 'settings.section.listing',
  Behavior: 'settings.section.behavior',
  'File operations': 'settings.section.fileOperations',
  'File system watching': 'settings.section.fileSystemWatching',
  Search: 'settings.section.search',
  AI: 'settings.section.ai',
  'File systems': 'settings.section.fileSystems',
  'SMB/Network shares': 'settings.section.smbNetworkShares',
  'MTP (Android/Kindle/cameras)': 'settings.section.mtp',
  Git: 'settings.section.git',
  Viewer: 'settings.section.viewer',
  Developer: 'settings.section.developer',
  'MCP server': 'settings.section.mcpServer',
  Logging: 'settings.section.logging',
  'Updates & privacy': 'settings.section.updatesAndPrivacy',
  Advanced: 'settings.section.advanced',
  'Keyboard shortcuts': 'settings.section.keyboardShortcuts',
  License: 'settings.section.license',
}

/** The translated title for a section name (falls back to the name itself). */
export function sectionTitle(name: string): string {
  const key = SECTION_TITLE_KEY[name]
  return key ? tString(key) : name
}
