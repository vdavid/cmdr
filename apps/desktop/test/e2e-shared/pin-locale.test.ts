/**
 * The locale pin every harness applies before launching the app.
 *
 * The merge behavior is the part worth covering: the marketing-shots instance
 * keeps a hand-adjusted `settings.json` between runs, so a pin that rewrote the
 * file would silently throw away David's pane paths, tabs, and favorites. The
 * argument half is covered too, since a typo there fails silently: macOS ignores
 * an argument it doesn't recognize, and the app just keeps the machine's locale.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import fs from 'fs'
import os from 'os'
import path from 'path'
import { EN_US_LOCALE_ARGS, pinUiLanguage } from './pin-locale.js'

let dataDir: string

beforeEach(() => {
  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-pin-locale-test-'))
})

afterEach(() => {
  fs.rmSync(dataDir, { recursive: true, force: true })
})

/** The instance's settings, as the app would read them back. */
function settingsOnDisk(): Record<string, unknown> {
  return JSON.parse(fs.readFileSync(path.join(dataDir, 'settings.json'), 'utf-8')) as Record<string, unknown>
}

describe('pinUiLanguage', () => {
  it('writes the pin into a data dir that has no settings yet', () => {
    pinUiLanguage(dataDir)

    expect(settingsOnDisk()['appearance.language']).toBe('en')
  })

  it('keeps every other setting the instance already had', () => {
    fs.writeFileSync(
      path.join(dataDir, 'settings.json'),
      JSON.stringify({ isOnboarded: true, 'appearance.appColor': 'cmdr-gold' }),
    )

    pinUiLanguage(dataDir)

    const settings = settingsOnDisk()
    expect(settings.isOnboarded).toBe(true)
    expect(settings['appearance.appColor']).toBe('cmdr-gold')
    expect(settings['appearance.language']).toBe('en')
  })

  it('overrides a language a previous run left behind', () => {
    fs.writeFileSync(path.join(dataDir, 'settings.json'), JSON.stringify({ 'appearance.language': 'hu' }))

    pinUiLanguage(dataDir)

    expect(settingsOnDisk()['appearance.language']).toBe('en')
  })

  it('replaces a settings file that is not readable JSON', () => {
    fs.writeFileSync(path.join(dataDir, 'settings.json'), '{ truncated mid-write')

    pinUiLanguage(dataDir)

    expect(settingsOnDisk()['appearance.language']).toBe('en')
  })

  it('creates a data dir that does not exist yet', () => {
    const fresh = path.join(dataDir, 'not-created-yet')

    pinUiLanguage(fresh)

    expect(JSON.parse(fs.readFileSync(path.join(fresh, 'settings.json'), 'utf-8'))).toEqual({
      'appearance.language': 'en',
    })
  })
})

describe('EN_US_LOCALE_ARGS', () => {
  it('names both halves of the OS answer, in NSUserDefaults argument form', () => {
    // `AppleLocale` carries the region override the formatters follow;
    // `AppleLanguages` takes a parenthesized list, not a bare tag.
    expect([...EN_US_LOCALE_ARGS]).toEqual(['-AppleLocale', 'en_US', '-AppleLanguages', '(en-US)'])
  })
})
