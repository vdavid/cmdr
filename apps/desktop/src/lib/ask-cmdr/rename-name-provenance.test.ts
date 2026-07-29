import { describe, expect, it } from 'vitest'
import { nameProvenance } from './rename-name-provenance'
import type { RenameEvidenceSource } from '$lib/tauri-commands'

function row(source: RenameEvidenceSource, sourceName = 'IMG_4021.png', destinationName = 'Receipt.png') {
  return { evidence: { source, detail: 'whatever' }, sourceName, destinationName }
}

describe('nameProvenance', () => {
  /** The two image sources are the only ones the backend checked against delivered content. */
  it('calls a name backed by the file’s contents read', () => {
    expect(nameProvenance(row('imageText'))).toBe('contentRead')
    expect(nameProvenance(row('imageTags'))).toBe('contentRead')
  })

  /**
   * The state M2 adds the column for: nothing inside the file was read, so the name rests on
   * the old name, the dates, or the user's own instruction. It has to be visible per row, not
   * only inferable from the evidence label.
   */
  it('marks a name that rests on nothing inside the file', () => {
    expect(nameProvenance(row('filename'))).toBe('nothingRead')
    expect(nameProvenance(row('metadata'))).toBe('nothingRead')
    expect(nameProvenance(row('userInstruction'))).toBe('nothingRead')
  })

  /**
   * The "kept the name" path M4 will instruct the model to take when it couldn't read a file.
   * The user has to be able to see which rows took it, and it must still say that nothing
   * inside the file was read.
   */
  it('marks a kept name as kept, and only when nothing was read', () => {
    expect(nameProvenance(row('metadata', 'IMG_4021.png', 'IMG_4021.png'))).toBe('nameKept')
    expect(nameProvenance(row('imageText', 'IMG_4021.png', 'IMG_4021.png'))).toBe(
      'contentRead',
      // Cmdr read the file and decided the name was already right: nothing to warn about.
    )
  })

  /** A case-only edit is a rename, not a kept name; the write engine stages it as one. */
  it('treats a case-only change as a rename, not a kept name', () => {
    expect(nameProvenance(row('metadata', 'img_4021.png', 'IMG_4021.png'))).toBe('nothingRead')
  })

  /** A name the user typed is the user's own decision, and claims nothing at all. */
  it('keeps a user-typed name in its own state', () => {
    expect(nameProvenance(row('userEdited'))).toBe('userEdited')
    expect(nameProvenance(row('userEdited', 'IMG_4021.png', 'IMG_4021.png'))).toBe('userEdited')
  })
})
