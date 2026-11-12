/**
 * The reason under a suggestion whose approval didn't start. The write-error pipeline itself is
 * covered in `../file-operations/transfer/transfer-error-messages.test.ts`; these pin that a
 * suggestion reads exactly as the clicked operation would, verb for verb.
 */

import { describe, it, expect } from 'vitest'
import type { ServerProposalView, WriteOperationError } from '#lib/ipc/bindings.js'
import { getUserFriendlyMessage } from '#lib/file-operations/transfer/transfer-error-messages.js'
import { wordConnectRefusal } from '#lib/servers/connect-refusals.js'
import { refusalReason } from './suggested-ops-refusal'
import type { ServerRefusal } from './suggested-ops-trigger.svelte'

const sourceGone: WriteOperationError = { type: 'source_not_found', path: '/Users/someone/Downloads/one.dmg' }

describe('refusalReason', () => {
  it('words an engine refusal exactly as the clicked operation would', () => {
    const reason = refusalReason({ kind: 'refused', error: sourceGone }, 'trash')
    const clicked = getUserFriendlyMessage(sourceGone, 'trash')

    expect(reason).toEqual({ message: clicked.message, suggestion: clicked.suggestion })
  })

  it('reads a rename as the move it runs as, and an extract as the copy it is', () => {
    expect(refusalReason({ kind: 'refused', error: sourceGone }, 'rename').message).toBe(
      getUserFriendlyMessage(sourceGone, 'move').message,
    )
    expect(refusalReason({ kind: 'refused', error: sourceGone }, 'extract').message).toBe(
      getUserFriendlyMessage(sourceGone, 'copy').message,
    )
  })

  it('says a phone or server nobody connected is not connected yet, never that it left', () => {
    const reason = refusalReason({ kind: 'refused', error: { type: 'source_not_connected', path: '/DCIM' } }, 'move')

    expect(reason.message).toContain('hasn’t connected')
  })

  it('has its own words for the refusals only suggestions have', () => {
    expect(refusalReason({ kind: 'nothingToRun' }, 'trash').message).toContain('nothing ran')
    expect(refusalReason({ kind: 'nothingSelected' }, 'move').message).toContain('nothing ran')
    expect(refusalReason({ kind: 'couldNotStart' }, 'move').message).toContain('nothing ran')
  })
})

describe('a server card’s refusal', () => {
  const card: ServerProposalView = {
    protocol: 'sftp',
    fields: [
      { field: 'address', value: 'build.local', changed: false, previous: null },
      { field: 'account', value: 'test_user_1', changed: false, previous: null },
    ],
    addressIsNew: true,
    sharesAffected: null,
  }
  const reason = (answer: ServerRefusal) => refusalReason({ kind: 'server', answer }, 'addServer', card)

  it('words a refused save exactly as the servers sheet would', () => {
    const subject = { host: 'build.local', username: 'test_user_1', protocol: 'sftp' as const }

    expect(reason({ kind: 'refused', outcome: { outcome: 'unreachable' } }).message).toBe(
      wordConnectRefusal('save_unconfirmed', subject),
    )
    expect(reason({ kind: 'refused', outcome: { outcome: 'address_taken', name: 'Build box' } }).message).toBe(
      wordConnectRefusal('address_taken', { ...subject, takenBy: 'Build box' }),
    )
  })

  it('words an SMB add the address refused as the add sheet does', () => {
    expect(reason({ kind: 'addServer', error: { type: 'invalid_address', message: 'x' } }).message).toBe(
      wordConnectRefusal('invalid_url', { host: 'build.local', username: 'test_user_1' }),
    )
  })

  it('has its own words for what only a suggested server meets', () => {
    expect(reason({ kind: 'serverChanged' }).message).toContain('changed since the AI suggested')
    expect(reason({ kind: 'alreadySaved', serverId: 'sftp-1' }).message).toContain('already in your servers')
    expect(reason({ kind: 'nameTaken', name: 'Testnas' }).message).toContain('Testnas')
  })

  // The message renders as HTML, the boundary the file refusals share: a name is text.
  it('escapes a name it says', () => {
    expect(reason({ kind: 'nameTaken', name: '<b>NAS</b>' }).message).toContain('&lt;b&gt;NAS&lt;/b&gt;')
    expect(reason({ kind: 'refused', outcome: { outcome: 'share_mounted', name: '<i>x</i>' } }).message).toContain(
      '&lt;i&gt;x&lt;/i&gt;',
    )
  })

  it('says what to do next only where the sentence itself doesn’t', () => {
    expect(reason({ kind: 'serverChanged' }).suggestion).not.toBeNull()
    expect(reason({ kind: 'refused', outcome: { outcome: 'start_folder_outside_root' } }).suggestion).toBeNull()
  })
})
