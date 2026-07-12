import { describe, it, expect } from 'vitest'
import { attachmentBasename } from './ask-cmdr-attachments'

describe('attachmentBasename', () => {
  it('returns the last segment of a file path', () => {
    expect(attachmentBasename('/Users/d/taxes.pdf')).toBe('taxes.pdf')
  })

  it('returns a folder name, trimming a trailing slash', () => {
    expect(attachmentBasename('/Users/d/photos/')).toBe('photos')
  })

  it('handles a bare name with no separator', () => {
    expect(attachmentBasename('notes.txt')).toBe('notes.txt')
  })

  it('handles backslash separators', () => {
    expect(attachmentBasename('C:\\Users\\d\\report.docx')).toBe('report.docx')
  })

  it('falls back to the whole path when trimming would empty it', () => {
    expect(attachmentBasename('/')).toBe('/')
  })
})
