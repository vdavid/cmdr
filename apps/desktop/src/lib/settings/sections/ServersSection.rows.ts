/**
 * The searchable non-setting rows of `ServersSection.svelte`.
 *
 * ❗ The page has no CONTROL at all: its whole content is the trusted host keys
 * and certificates, one Forget per row. So the first row is also what puts the
 * page in the sidebar (`anchorsSection`). ❌ Never give the page a `hidden` registry entry to do that
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
    keywordsKey: 'settings.network.trustedHostKeys.keywords',
    terms: ['sftp', 'webdav', 'ssh'],
    anchorsSection: { after: 'SMB/Network shares' },
  },
  {
    // The trusted certificates card: the server certificates (a NAS's self-signed
    // one) a person told Cmdr to trust, each with the Forget that asks again.
    id: 'row:network.trustedCertificates',
    section: ['File systems', 'Servers (SFTP, WebDAV)'],
    labelKey: 'settings.servers.card.trustedCertificates',
    cardKey: 'settings.servers.card.trustedCertificates',
    keywordsKey: 'settings.network.trustedCertificates.keywords',
    terms: ['webdav', 'https', 'tls', 'ssl'],
  },
]
