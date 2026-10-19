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
    keywordsKey: 'settings.viewer.wordWrap.keywords',
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'viewer.showTextCursor',
    section: ['Viewer'],
    labelKey: 'settings.viewer.showTextCursor.label',
    descriptionKey: 'settings.viewer.showTextCursor.description',
    keywordsKey: 'settings.viewer.showTextCursor.keywords',
    type: 'boolean',
    // Off by design: the viewer is for looking at a file, not editing one, so a
    // blinking bar would be noise for most people. The setting is here for those
    // who expect an editor.
    default: false,
    component: 'switch',
  },
]
