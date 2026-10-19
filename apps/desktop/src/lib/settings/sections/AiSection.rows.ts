/**
 * The searchable non-setting rows of `AiSection.svelte`.
 *
 * The Allow cloud AI switch isn't a setting: the consent it records lives in `main.db`
 * (`#lib/ai/cloud-consent.svelte.ts`), so it's findable through this row. It renders on Cloud
 * only, like the service rows registered beside it in `definitions/ai.ts`.
 */

import type { SearchableRow } from '../types'

export const aiRows: SearchableRow[] = [
  {
    id: 'row:ai.cloudConsent',
    section: ['AI', 'Provider'],
    labelKey: 'ai.cloudConsent.label',
    keywordsKey: 'settings.ai.cloudConsent.keywords',
  },
]
