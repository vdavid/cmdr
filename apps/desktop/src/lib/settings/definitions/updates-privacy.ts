/**
 * Updates & privacy section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 *
 * `whatsNew.lastSeenVersion` lives here (its `section` is `['Advanced']`, so it
 * renders on the Advanced page): it's a hidden state flag colocated with the
 * `whatsNew.showOnUpdate` setting it pairs with, and its authored position in the
 * registry is preserved so the full array order stays byte-for-byte identical.
 */

import type { SettingDefinitionSource } from '../types'

export const updatesPrivacySettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Updates & privacy
  // ========================================================================
  {
    id: 'updates.autoCheck',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.updates',
    labelKey: 'settings.updates.autoCheck.label',
    descriptionKey: 'settings.updates.autoCheck.description',
    keywords: ['update', 'auto', 'check', 'version', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'whatsNew.showOnUpdate',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.updates',
    labelKey: 'settings.whatsNew.showOnUpdate.label',
    descriptionKey: 'settings.whatsNew.showOnUpdate.description',
    keywords: ['changelog', 'release notes', "what's new", 'update notes'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'whatsNew.lastSeenVersion',
    section: ['Advanced'],
    labelKey: 'settings.whatsNew.lastSeenVersion.label',
    descriptionKey: 'settings.whatsNew.lastSeenVersion.description',
    keywords: [],
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
  },
  {
    id: 'analytics.enabled',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.privacyAndDataSharing',
    labelKey: 'settings.analytics.enabled.label',
    descriptionKey: 'settings.analytics.enabled.description',
    keywords: ['analytics', 'usage', 'stats', 'privacy', 'telemetry', 'beta', 'tracking', 'opt-out'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'analytics.email',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.privacyAndDataSharing',
    labelKey: 'settings.analytics.email.label',
    descriptionKey: 'settings.analytics.email.description',
    keywords: ['email', 'beta', 'contact', 'newsletter', 'updates', 'survey'],
    type: 'string',
    default: '',
    component: 'text-input',
  },
  {
    id: 'updates.crashReports',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.privacyAndDataSharing',
    labelKey: 'settings.updates.crashReports.label',
    descriptionKey: 'settings.updates.crashReports.description',
    keywords: ['crash', 'report', 'privacy', 'telemetry', 'bug', 'error'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'updates.errorReports',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.privacyAndDataSharing',
    labelKey: 'settings.updates.errorReports.label',
    descriptionKey: 'settings.updates.errorReports.description',
    keywords: ['error', 'report', 'auto', 'send', 'privacy', 'telemetry', 'bug', 'log', 'snippet', 'diagnostics'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
]
