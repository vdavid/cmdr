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
    keywordsKey: 'settings.updates.autoCheck.keywords',
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
    keywordsKey: 'settings.whatsNew.showOnUpdate.keywords',
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'whatsNew.lastSeenVersion',
    section: ['Advanced'],
    labelKey: 'settings.whatsNew.lastSeenVersion.label',
    descriptionKey: 'settings.whatsNew.lastSeenVersion.description',
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
    keywordsKey: 'settings.analytics.enabled.keywords',
    type: 'boolean',
    default: true,
    component: 'switch',
    // A consent answer: an MCP client may help someone opt out, but only the person opts in.
    mcpSettable: 'offOnly',
  },
  {
    id: 'analytics.email',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.privacyAndDataSharing',
    labelKey: 'settings.analytics.email.label',
    descriptionKey: 'settings.analytics.email.description',
    keywordsKey: 'settings.analytics.email.keywords',
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
    keywordsKey: 'settings.updates.crashReports.keywords',
    type: 'boolean',
    // Default ON, unlike `updates.errorReports` right below. A crash report is narrow and
    // stack-shaped (app version, macOS version, where it stopped) and goes through
    // `sanitize_panic_message`; the error report is an unbounded log bundle. That difference is
    // the whole reason only one of the two defaults on. ❌ Don't fold them into one default.
    default: true,
    component: 'switch',
    // A consent answer: an MCP client may help someone opt out, but only the person opts in.
    mcpSettable: 'offOnly',
  },
  {
    id: 'updates.errorReports',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.privacyAndDataSharing',
    labelKey: 'settings.updates.errorReports.label',
    descriptionKey: 'settings.updates.errorReports.description',
    keywordsKey: 'settings.updates.errorReports.keywords',
    type: 'boolean',
    default: false,
    component: 'switch',
    // A consent answer: an MCP client may help someone opt out, but only the person opts in.
    mcpSettable: 'offOnly',
  },
  {
    id: 'securityLog.enabled',
    section: ['Updates & privacy'],
    cardKey: 'settings.updates.card.securityEvents',
    labelKey: 'settings.securityLog.enabled.label',
    descriptionKey: 'settings.securityLog.enabled.description',
    keywordsKey: 'settings.securityLog.enabled.keywords',
    terms: ['cra'],
    type: 'boolean',
    default: true,
    component: 'switch',
    // ❌ Not even off: an MCP client must not be able to switch off the record of what MCP clients do.
    mcpSettable: false,
  },
]
