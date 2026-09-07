/**
 * The searchable non-setting rows of `AdbSection.svelte`.
 *
 * The page itself comes from its two real settings; this is the status block
 * beside them, which no setting models. The Browse button isn't declared: it
 * lives inside the `fileOperations.adbBinaryPath` row and is findable through it.
 */

import type { SearchableRow } from '../types'

export const adbRows: SearchableRow[] = [
  {
    // "adb" plus where Cmdr found it (or that it didn't), whether phones are
    // being watched for, and the Re-check button. Rendered whenever the page is.
    id: 'row:adb.status',
    section: ['File systems', 'Android (ADB)'],
    labelKey: 'settings.adb.status.label',
    keywords: ['adb', 'android', 'status', 're-check', 'recheck', 'found', 'watching', 'platform-tools', 'install'],
  },
]
