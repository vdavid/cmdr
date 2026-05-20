import { describe, it, expect } from 'vitest'

import { categorizeForViewerWarning } from './binary-warning'

describe('categorizeForViewerWarning', () => {
  describe('image extensions → "image"', () => {
    it.each(['photo.jpg', 'snap.JPEG', 'logo.png', 'anim.gif', 'pic.webp', 'shot.heic', 'icon.ico', 'art.avif'])(
      '%s → image',
      (name) => {
        expect(categorizeForViewerWarning(name)).toEqual({ shouldWarn: true, label: 'image' })
      },
    )
  })

  describe('document extensions → "document"', () => {
    it.each(['report.pdf', 'contract.docx', 'budget.xlsx', 'slides.pptx', 'notes.pages', 'data.numbers', 'novel.epub'])(
      '%s → document',
      (name) => {
        expect(categorizeForViewerWarning(name)).toEqual({ shouldWarn: true, label: 'document' })
      },
    )
  })

  describe('other binary extensions → uppercased extension', () => {
    it('installer.exe → EXE', () => {
      expect(categorizeForViewerWarning('installer.exe')).toEqual({ shouldWarn: true, label: 'EXE' })
    })
    it('archive.zip → ZIP', () => {
      expect(categorizeForViewerWarning('archive.zip')).toEqual({ shouldWarn: true, label: 'ZIP' })
    })
    it('video.mp4 → MP4', () => {
      expect(categorizeForViewerWarning('video.mp4')).toEqual({ shouldWarn: true, label: 'MP4' })
    })
    it('sound.mp3 → MP3', () => {
      expect(categorizeForViewerWarning('sound.mp3')).toEqual({ shouldWarn: true, label: 'MP3' })
    })
    it('font.woff2 → WOFF2', () => {
      expect(categorizeForViewerWarning('font.woff2')).toEqual({ shouldWarn: true, label: 'WOFF2' })
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
      expect(categorizeForViewerWarning(name)).toEqual({ shouldWarn: false, label: '' })
    })

    // Unknown extension we don't classify: better to under-warn than over-warn.
    it('random.xyz → no warning', () => {
      expect(categorizeForViewerWarning('random.xyz')).toEqual({ shouldWarn: false, label: '' })
    })
  })

  describe('edge cases', () => {
    it('files with no extension never warn (Makefile, README, etc.)', () => {
      expect(categorizeForViewerWarning('Makefile')).toEqual({ shouldWarn: false, label: '' })
      expect(categorizeForViewerWarning('README')).toEqual({ shouldWarn: false, label: '' })
    })

    it('hidden files with no real extension never warn (.bashrc, .gitignore)', () => {
      // ".bashrc" has the dot at index 0; we treat that as "no extension".
      expect(categorizeForViewerWarning('.bashrc')).toEqual({ shouldWarn: false, label: '' })
      expect(categorizeForViewerWarning('.gitignore')).toEqual({ shouldWarn: false, label: '' })
    })

    it('trailing dot is treated as no extension', () => {
      expect(categorizeForViewerWarning('name.')).toEqual({ shouldWarn: false, label: '' })
    })

    it('empty string → no warning, no crash', () => {
      expect(categorizeForViewerWarning('')).toEqual({ shouldWarn: false, label: '' })
    })

    it('extension is matched case-insensitively', () => {
      expect(categorizeForViewerWarning('PHOTO.JPG')).toEqual({ shouldWarn: true, label: 'image' })
      expect(categorizeForViewerWarning('Setup.EXE')).toEqual({ shouldWarn: true, label: 'EXE' })
    })

    it('multi-dot filenames use the last segment', () => {
      expect(categorizeForViewerWarning('archive.tar.gz')).toEqual({ shouldWarn: true, label: 'GZ' })
      expect(categorizeForViewerWarning('photo.thumbnail.png')).toEqual({ shouldWarn: true, label: 'image' })
    })
  })
})
