import { describe, expect, it } from 'vitest'
import { refuse, refuseWith } from './answer-types.js'
import { appAnswers, rawAnswers, silentCommands } from './answers.js'

type Handler = (payload: Record<string, unknown>) => unknown

/** A payload with every argument an app answer reads, so any of them can run. */
const anyPayload = { volumeId: 'root', paths: ['/a', '/b'], iconIds: ['dir', 'ext:pdf'], path: '/a' }

function ask(name: string, payload: Record<string, unknown> = anyPayload): unknown {
  const answer = (appAnswers as Record<string, Handler | undefined>)[name]
  if (!answer) throw new Error(`no answer for ${name}`)
  return answer(payload)
}

describe('app answers', () => {
  it('answer every command they list without throwing, except the typed refusals', () => {
    const refused = new Set(['openPath'])
    for (const [name, answer] of Object.entries(appAnswers as Record<string, Handler>)) {
      if (refused.has(name)) expect(() => answer(anyPayload)).toThrow()
      else expect(() => answer(anyPayload)).not.toThrow()
    }
  })

  it('run as a fresh personal-use Cmdr with no updater, index, or AI', () => {
    expect(ask('getLicenseStatus')).toEqual({ type: 'personal', showCommercialReminder: false })
    expect((ask('getManagedPolicy') as { updates: unknown }).updates).toEqual({ kind: 'disabled' })
    expect((ask('getIndexStatus') as { initialized: boolean }).initialized).toBe(false)
    expect((ask('getAiRuntimeStatus') as { serverRunning: boolean }).serverRunning).toBe(false)
    expect(ask('getAiStatus')).toBe('unavailable')
  })

  it('answer every icon with the bundled generic folder or document', () => {
    const icons = ask('getIcons', { iconIds: ['dir', 'symlink-dir', 'special:home', 'path:/x', 'ext:pdf'] }) as {
      data: Record<string, string>
    }
    expect(icons.data).toEqual({
      dir: '/icons/default-folder.png',
      'symlink-dir': '/icons/default-folder.png',
      'special:home': '/icons/default-folder.png',
      'path:/x': '/icons/default-folder.png',
      'ext:pdf': '/icons/default-file.png',
    })
  })

  it('refuse opening a file with the typed no-opener refusal', () => {
    expect(() => ask('openPath')).toThrow(expect.objectContaining({ type: 'unsupported' }) as Error)
    try {
      ask('openPath')
    } catch (error) {
      expect(error).toEqual({ type: 'unsupported' })
    }
  })

  it('volume index status names the asked volume', () => {
    expect((ask('getVolumeIndexStatusById', { volumeId: 'root' }) as { volumeId: string }).volumeId).toBe('root')
  })
})

describe('raw and silent answers', () => {
  it('answer the raw commands, configure_ai with no secret-store failure', () => {
    for (const [name, answer] of Object.entries(rawAnswers)) {
      expect(() => answer?.({}), name).not.toThrow()
    }
    expect(rawAnswers.configure_ai?.({})).toEqual({ secretStoreError: null })
  })

  it('list each silent command once', () => {
    expect(new Set(silentCommands).size).toBe(silentCommands.length)
  })
})

describe('refusals', () => {
  it('throw the typed value itself, never an Error', () => {
    expect(() => refuse('openPath', { type: 'timedOut' })).toThrow()
    try {
      refuseWith('plain refusal')
    } catch (error) {
      expect(error).toBe('plain refusal')
    }
  })
})
