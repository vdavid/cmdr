/**
 * The frontend mirror of the product's one extension rule
 * (`crates/cmdr-fs/src/file_extension.rs`). Both suites run the cases in
 * `file_extension_cases.json`, so the two languages can't drift apart.
 */

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { beforeEach, describe, expect, it } from 'vitest'
import { withCapabilities } from '#lib/platform/test-platform.js'
import {
  COMPOUND_EXTENSIONS,
  PACKAGE_EXTENSIONS,
  fileExtension,
  kindOfRenamedName,
  nameKindOf,
  nameWithoutExtension,
  withExtension,
  type NameKind,
} from './file-extension'

interface Spec {
  packageExtensions: string[]
  compoundExtensions: string[]
  cases: { name: string; kind: NameKind; extension: string }[]
}

const spec = JSON.parse(
  readFileSync(resolve(process.cwd(), '../../crates/cmdr-fs/src/file_extension_cases.json'), 'utf8'),
) as Spec

describe('fileExtension ↔ the backend`s `extension_of`', () => {
  it.each(spec.cases)('$name ($kind) → "$extension"', ({ name, kind, extension }) => {
    expect(fileExtension(name, kind)).toBe(extension)
  })

  it('knows exactly the compound extensions the shared spec does', () => {
    expect([...COMPOUND_EXTENSIONS]).toEqual(spec.compoundExtensions)
  })

  it('knows exactly the package extensions the shared spec (and so Rust) does', () => {
    expect([...PACKAGE_EXTENSIONS].sort()).toEqual([...spec.packageExtensions].sort())
  })
})

// Packages by name are macOS's (`appBundles`); the cases below are about that rule.
beforeEach(() => withCapabilities('appBundles'))

describe('kindOfRenamedName', () => {
  it('keeps a file a file and a package a package', () => {
    expect(kindOfRenamedName('file', 'Thing.app')).toBe('file')
    expect(kindOfRenamedName('package', 'Cmdr')).toBe('package')
    expect(kindOfRenamedName('package', 'Sketch.other')).toBe('package')
  })

  it('makes a folder a package only by a listed suffix', () => {
    expect(kindOfRenamedName('folder', 'Plugins.bundle')).toBe('package')
    expect(kindOfRenamedName('folder', 'Plugins.BUNDLE')).toBe('package')
    expect(kindOfRenamedName('folder', 'release.v1.3')).toBe('folder')
    expect(kindOfRenamedName('folder', '.app')).toBe('folder')
  })
})

describe('off macOS, where no folder name makes a package', () => {
  beforeEach(() => withCapabilities())

  it('keeps a bundle-suffixed folder a folder', () => {
    expect(kindOfRenamedName('folder', 'Plugins.bundle')).toBe('folder')
    expect(nameKindOf({ name: 'Cmdr.app', isDirectory: true })).toBe('folder')
  })
})

describe('nameKindOf', () => {
  it('reads packageness off the entry when it says', () => {
    expect(nameKindOf({ name: 'Cmdr.app', isDirectory: false, isPackage: false })).toBe('file')
    expect(nameKindOf({ name: 'Cmdr.app', isDirectory: true, isPackage: false })).toBe('folder')
    expect(nameKindOf({ name: 'Logo.sketchbundle', isDirectory: true, isPackage: true })).toBe('package')
  })

  it('falls back to the package list for an entry that does not say', () => {
    expect(nameKindOf({ name: 'Cmdr.app', isDirectory: true })).toBe('package')
    expect(nameKindOf({ name: 'Logo.sketchbundle', isDirectory: true })).toBe('folder')
    expect(nameKindOf({ name: 'Cmdr.app', isDirectory: false })).toBe('file')
  })
})

describe('withExtension', () => {
  it('swaps, adds, and removes an extension', () => {
    expect(withExtension('photo.png', 'jpg', 'file')).toBe('photo.jpg')
    expect(withExtension('photo', 'jpg', 'file')).toBe('photo.jpg')
    expect(withExtension('README.md', '', 'file')).toBe('README')
    expect(withExtension('src.tar.gz', 'zip', 'file')).toBe('src.zip')
    expect(withExtension('Smlouva v. 2 final', 'pdf', 'file')).toBe('Smlouva v. 2 final.pdf')
    expect(withExtension('Cmdr.old', 'app', 'package')).toBe('Cmdr.app')
  })
})

describe('nameWithoutExtension', () => {
  it('strips the extension and its dot, and nothing when there is none', () => {
    expect(nameWithoutExtension('report.pdf', 'file')).toBe('report')
    expect(nameWithoutExtension('src.tar.gz', 'file')).toBe('src')
    expect(nameWithoutExtension('Smlouva v. 2 final', 'file')).toBe('Smlouva v. 2 final')
    expect(nameWithoutExtension('release.v1.2', 'folder')).toBe('release.v1.2')
    expect(nameWithoutExtension('Cmdr.app', 'package')).toBe('Cmdr')
    expect(nameWithoutExtension('Sketch.myBundle', 'package')).toBe('Sketch')
    expect(nameWithoutExtension('.gitignore', 'file')).toBe('.gitignore')
  })
})
