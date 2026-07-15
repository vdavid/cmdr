/**
 * Viewer section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'

export const viewerSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Viewer
  // ========================================================================
  {
    id: 'viewer.wordWrap',
    section: ['Viewer'],
    labelKey: 'settings.viewer.wordWrap.label',
    descriptionKey: 'settings.viewer.wordWrap.description',
    keywords: ['viewer', 'wrap', 'word', 'line', 'horizontal', 'scroll'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
]
