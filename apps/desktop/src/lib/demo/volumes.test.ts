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
  it('lists the main volume and the favorites, all writable and without a Trash', () => {
    const { data } = ask('listVolumes') as {
      data: { id: string; category: string; mountIsReadOnly: boolean; supportsTrash: boolean }[]
    }
    expect(data.find((v) => v.category === 'main_volume')?.id).toBe(DEMO_VOLUME_ID)
    expect(data.filter((v) => v.category === 'favorite').length).toBeGreaterThan(0)
    // Writes go to the real engine; F8 takes the permanent-delete confirm.
    expect(data.every((v) => !v.mountIsReadOnly && !v.supportsTrash)).toBe(true)
  })

  it('leaves the path questions to the engine, which sees every write', () => {
    expect(volumeAnswers.pathExists).toBeUndefined()
    expect(volumeAnswers.statPathsKinds).toBeUndefined()
  })

  it('answers the default volume and resolves any path to the main one', () => {
    expect(ask('getDefaultVolumeId')).toBe(DEMO_VOLUME_ID)
    expect(ask('getE2eStartPath')).toBeNull()
    expect((ask('resolvePathVolume', { path: '/' }) as { volume: { id: string } }).volume.id).toBe(DEMO_VOLUME_ID)
  })

  it('reports nothing syncing and every path stored as given', () => {
    expect(ask('getSyncStatus', { paths: ['~/notes.md'] })).toEqual({ data: {}, timedOut: false })
    expect(ask('storedSpellings', { volumeId: DEMO_VOLUME_ID, paths: ['/a', '/b'] })).toEqual({
      data: ['/a', '/b'],
      timedOut: false,
    })
  })
})
