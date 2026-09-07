/**
 * The searchable non-setting rows of `ServersSection.svelte`.
 *
 * ❗ The page has no CONTROL at all: its whole content is the trusted-host-key
 * list, one Forget per row. So this row is also what puts the page in the sidebar
 * (`anchorsSection`). ❌ Never give the page a `hidden` registry entry to do that
 * instead: it would be a `SettingsValues` key nothing reads.
 */

import type { SearchableRow } from '../types'

export const serversRows: SearchableRow[] = [
  {
    // The trusted host keys card: host and port, algorithm, fingerprint, when it
    // was trusted, and the Forget that makes the next connection first contact.
    id: 'row:network.trustedHostKeys',
    section: ['File systems', 'Servers (SFTP, WebDAV)'],
    labelKey: 'settings.servers.card.trustedHostKeys',
    cardKey: 'settings.servers.card.trustedHostKeys',
    keywords: [
      'sftp',
      'webdav',
      'ssh',
      'host key',
      'fingerprint',
      'trust',
      'trusted',
      'known hosts',
      'server',
      'forget',
    ],
    anchorsSection: { after: 'SMB/Network shares' },
  },
]
