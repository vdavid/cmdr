/**
 * Tests for `paneOffersTrash`, the predicate both destructive-choice surfaces ask
 * before offering "…and trash the old one": the delete dialog and the rename
 * conflict dialog. Pure, no mocks.
 */

import { describe, it, expect } from 'vitest'
import { paneOffersTrash } from './trash-availability'
import type { VolumeInfo } from '../types'

/** A volume row with only the fields this predicate reads. */
const volume = (id: string, supportsTrash: boolean) => ({ id, supportsTrash }) as VolumeInfo

const LOCAL = volume('root', true)
const SFTP = volume('sftp-nas-local-22-ada-9f3c', false)

describe('paneOffersTrash', () => {
  it('offers the trash on a volume that has one', () => {
    expect(paneOffersTrash('root', '/Users/ada/notes', [LOCAL])).toBe(true)
  })

  it('refuses on a volume that says it has none', () => {
    expect(paneOffersTrash(SFTP.id, 'sftp://ada@nas.local:22/srv/data', [LOCAL, SFTP])).toBe(false)
  })

  it('refuses at or inside an archive, whatever the drive underneath says', () => {
    // The backend rejects trashing an archive-inner path, and there is no
    // `.Trash` inside a zip to reject into.
    expect(paneOffersTrash('root', '/Users/ada/backup.zip', [LOCAL])).toBe(false)
    expect(paneOffersTrash('root', '/Users/ada/backup.zip/photos', [LOCAL])).toBe(false)
  })

  it('guesses YES for a volume that has left the list', () => {
    // Optimistic on purpose, matching `resolveSnapshotSourceVolume`: a trash the
    // backend can't perform fails honestly and reversibly, while a `false` here
    // would push the surface into a permanent delete nothing can undo.
    expect(paneOffersTrash('smb-nas-445-media', '/Volumes/media/clips', [LOCAL])).toBe(true)
  })
})
