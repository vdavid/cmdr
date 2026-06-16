import { describe, it, expect, beforeAll, afterAll } from 'vitest'

import { categorizeForViewerWarning, viewerWarningLabel } from './binary-warning'
import { _setLocaleForTests } from '$lib/intl/locale'

describe('categorizeForViewerWarning', () => {
  describe('image extensions → image category', () => {
    it.each(['photo.jpg', 'snap.JPEG', 'logo.png', 'anim.gif', 'pic.webp', 'shot.heic', 'icon.ico', 'art.avif'])(
      '%s → image',
      (name) => {
        expect(categorizeForViewerWarning(name)).toEqual({ shouldWarn: true, category: 'image', ext: '' })
      },
    )
  })

  describe('document extensions → document category', () => {
    it.each(['report.pdf', 'contract.docx', 'budget.xlsx', 'slides.pptx', 'notes.pages', 'data.numbers', 'novel.epub'])(
      '%s → document',
      (name) => {
        expect(categorizeForViewerWarning(name)).toEqual({ shouldWarn: true, category: 'document', ext: '' })
      },
    )
  })

  describe('other binary extensions → binary category with uppercased extension', () => {
    it('installer.exe → EXE', () => {
      expect(categorizeForViewerWarning('installer.exe')).toEqual({ shouldWarn: true, category: 'binary', ext: 'EXE' })
    })
    it('archive.zip → ZIP', () => {
      expect(categorizeForViewerWarning('archive.zip')).toEqual({ shouldWarn: true, category: 'binary', ext: 'ZIP' })
    })
    it('video.mp4 → MP4', () => {
      expect(categorizeForViewerWarning('video.mp4')).toEqual({ shouldWarn: true, category: 'binary', ext: 'MP4' })
    })
    it('sound.mp3 → MP3', () => {
      expect(categorizeForViewerWarning('sound.mp3')).toEqual({ shouldWarn: true, category: 'binary', ext: 'MP3' })
    })
    it('font.woff2 → WOFF2', () => {
      expect(categorizeForViewerWarning('font.woff2')).toEqual({ shouldWarn: true, category: 'binary', ext: 'WOFF2' })
    })
  })

  describe('text-like or unknown extensions do NOT warn', () => {
    // Plain text and source code: showing raw bytes is the point.
    it.each([
      'README.md',
      'notes.txt',
      'config.json',
      'data.csv',
      'app.ts',
      'main.rs',
      'script.py',
      'styles.css',
      'index.html',
      'icon.svg', // text-based XML
      'log.log',
      'Cargo.toml',
      'Dockerfile.yaml',
    ])('%s → no warning', (name) => {
      expect(categorizeForViewerWarning(name)).toEqual({ shouldWarn: false, category: null, ext: '' })
    })

    // Unknown extension we don't classify: better to under-warn than over-warn.
    it('random.xyz → no warning', () => {
      expect(categorizeForViewerWarning('random.xyz')).toEqual({ shouldWarn: false, category: null, ext: '' })
    })
  })

  describe('edge cases', () => {
    it('files with no extension never warn (Makefile, README, etc.)', () => {
      expect(categorizeForViewerWarning('Makefile')).toEqual({ shouldWarn: false, category: null, ext: '' })
      expect(categorizeForViewerWarning('README')).toEqual({ shouldWarn: false, category: null, ext: '' })
    })

    it('hidden files with no real extension never warn (.bashrc, .gitignore)', () => {
      // ".bashrc" has the dot at index 0; we treat that as "no extension".
      expect(categorizeForViewerWarning('.bashrc')).toEqual({ shouldWarn: false, category: null, ext: '' })
      expect(categorizeForViewerWarning('.gitignore')).toEqual({ shouldWarn: false, category: null, ext: '' })
    })

    it('trailing dot is treated as no extension', () => {
      expect(categorizeForViewerWarning('name.')).toEqual({ shouldWarn: false, category: null, ext: '' })
    })

    it('empty string → no warning, no crash', () => {
      expect(categorizeForViewerWarning('')).toEqual({ shouldWarn: false, category: null, ext: '' })
    })

    it('extension is matched case-insensitively', () => {
      expect(categorizeForViewerWarning('PHOTO.JPG')).toEqual({ shouldWarn: true, category: 'image', ext: '' })
      expect(categorizeForViewerWarning('Setup.EXE')).toEqual({ shouldWarn: true, category: 'binary', ext: 'EXE' })
    })

    it('multi-dot filenames use the last segment', () => {
      expect(categorizeForViewerWarning('archive.tar.gz')).toEqual({ shouldWarn: true, category: 'binary', ext: 'GZ' })
      expect(categorizeForViewerWarning('photo.thumbnail.png')).toEqual({
        shouldWarn: true,
        category: 'image',
        ext: '',
      })
    })
  })
})

describe('viewerWarningLabel (en)', () => {
  beforeAll(() => {
    _setLocaleForTests('en-US')
  })
  afterAll(() => {
    _setLocaleForTests(null)
  })

  it('resolves the translatable lowercase words for image and document', () => {
    expect(viewerWarningLabel(categorizeForViewerWarning('photo.jpg'))).toBe('image')
    expect(viewerWarningLabel(categorizeForViewerWarning('report.pdf'))).toBe('document')
  })

  it('passes the uppercased extension through for the generic-binary case', () => {
    expect(viewerWarningLabel(categorizeForViewerWarning('archive.zip'))).toBe('ZIP')
  })

  it('returns an empty string for a non-warning result', () => {
    expect(viewerWarningLabel(categorizeForViewerWarning('notes.txt'))).toBe('')
  })
})
