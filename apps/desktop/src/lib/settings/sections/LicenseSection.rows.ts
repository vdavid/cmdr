/**
 * The searchable non-setting rows of `LicenseSection.svelte`.
 *
 * The License page models nothing in the registry, so before these rows the
 * sidebar could light up for "license" (a hardcoded keyword list in
 * `getMatchingSections`) while the content pane stayed empty: `SettingsContent`
 * gates the page on the section-scoped match set, which had nothing in it.
 *
 * The page never filters its own content (it's one card of read-only license
 * facts plus the key actions), so every hit lands on a rendered card. That's why
 * the mutually-exclusive "Manage license key" / "Enter license key" pair is both
 * registered: only one renders at a time, but either way the searcher arrives at
 * the card that holds it.
 */

import type { SearchableRow } from '../types'

export const licenseRows: SearchableRow[] = [
  {
    // Shown when a license is installed; opens the license-key dialog.
    id: 'row:license.manageKey',
    section: ['License'],
    labelKey: 'licensing.section.manageKey',
    keywords: ['license', 'key', 'manage', 'activate', 'activation', 'change'],
  },
  {
    // The same button unlicensed, where it reads "Enter license key".
    id: 'row:license.enterKey',
    section: ['License'],
    labelKey: 'licensing.section.enterKey',
    keywords: ['license', 'key', 'enter', 'activate', 'activation', 'redeem'],
  },
  {
    id: 'row:license.getLicense',
    section: ['License'],
    labelKey: 'licensing.section.getLicense',
    keywords: ['license', 'buy', 'purchase', 'pricing', 'commercial', 'personal', 'upgrade'],
  },
]
