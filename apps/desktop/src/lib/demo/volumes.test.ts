import { describe, expect, it } from 'vitest'
import { DEMO_HOME, DEMO_VOLUME_ID } from './demo-tree.js'
import { demoPath, volumeAnswers } from './volumes.js'

/** Calls one answer by its bindings name. */
function ask(name: keyof typeof volumeAnswers, payload: Record<string, unknown> = {}): unknown {
  const answer = volumeAnswers[name] as ((p: Record<string, unknown>) => unknown) | undefined
  if (!answer) throw new Error(`no answer for ${name}`)
  return answer(payload)
}

describe('demoPath', () => {
  it('expands ~ to the demo home', () => {
    expect(demoPath('~')).toBe(DEMO_HOME)
    expect(demoPath('~/Documents')).toBe(`${DEMO_HOME}/Documents`)
  })

  it('drops a trailing slash but keeps the root', () => {
    expect(demoPath(`${DEMO_HOME}/Documents/`)).toBe(`${DEMO_HOME}/Documents`)
    expect(demoPath('/')).toBe('/')
  })

  it('leaves a plain absolute path and a ~ inside a name alone', () => {
    expect(demoPath('/Users/you/notes.md')).toBe('/Users/you/notes.md')
    expect(demoPath('/tmp/~backup')).toBe('/tmp/~backup')
  })
})

describe('volume answers', () => {
  it('lists the main volume and the favorites, all read-only', () => {
    const { data } = ask('listVolumes') as { data: { id: string; category: string; mountIsReadOnly: boolean }[] }
    expect(data.find((v) => v.category === 'main_volume')?.id).toBe(DEMO_VOLUME_ID)
    expect(data.filter((v) => v.category === 'favorite').length).toBeGreaterThan(0)
    // The demo writes nothing; the read-only mount is how the app learns it (`writes.ts`).
    expect(data.every((v) => v.mountIsReadOnly)).toBe(true)
  })

  it('answers the default volume and resolves any path to the main one', () => {
    expect(ask('getDefaultVolumeId')).toBe(DEMO_VOLUME_ID)
    expect(ask('getE2eStartPath')).toBeNull()
    expect((ask('resolvePathVolume', { path: '/' }) as { volume: { id: string } }).volume.id).toBe(DEMO_VOLUME_ID)
  })

  it('knows which paths exist, ~ and trailing slashes included', () => {
    expect(ask('pathExists', { path: '~/Documents/' })).toEqual({ data: true, timedOut: false })
    expect(ask('pathExists', { path: '/' })).toEqual({ data: true, timedOut: false })
    expect(ask('pathExists', { path: '~/Nope' })).toEqual({ data: false, timedOut: false })
  })

  it('tells folders from files, and unknown paths as null', () => {
    const paths = ['~/Documents', '~/notes.md', '~/missing.txt']
    expect(ask('statPathsKinds', { paths })).toEqual({ data: [true, false, null], timedOut: false })
  })

  it('reports nothing syncing and every path stored as given', () => {
    expect(ask('getSyncStatus', { paths: ['~/notes.md'] })).toEqual({ data: {}, timedOut: false })
    expect(ask('storedSpellings', { volumeId: DEMO_VOLUME_ID, paths: ['/a', '/b'] })).toEqual({
      data: ['/a', '/b'],
      timedOut: false,
    })
  })
})
