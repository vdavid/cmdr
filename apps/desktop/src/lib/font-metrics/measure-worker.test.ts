// The worker shell: it owns an OffscreenCanvas and answers measure requests.
//
// Driven directly rather than through a real `Worker`: the module assigns
// `self.onmessage`, so importing it here registers the handler on the test
// global and we can post to it by hand.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { measureCodePoints } from './measure'
import type { MeasureRequest, MeasureResponse } from './measure-worker'

const spec = { fontFamily: 'Menlo', fontWeight: 400, fontSize: 12 }

/** Installs a fake `OffscreenCanvas` whose context reports UTF-16 length. */
function installCanvas(options: { contextAvailable: boolean } = { contextAvailable: true }) {
  class FakeOffscreenCanvas {
    getContext(kind: string) {
      if (!options.contextAvailable || kind !== '2d') return null
      return {
        font: '',
        measureText: (text: string) => ({ width: text.length }),
      }
    }
  }
  vi.stubGlobal('OffscreenCanvas', FakeOffscreenCanvas)
}

/** Imports the worker fresh and captures what it posts back. */
async function loadWorker() {
  const posted: MeasureResponse[] = []
  vi.stubGlobal('postMessage', (message: MeasureResponse) => posted.push(message))

  vi.resetModules()
  await import('./measure-worker')

  const handler = (globalThis as unknown as { onmessage: (event: MessageEvent<MeasureRequest>) => void }).onmessage
  return {
    posted,
    send: (request: MeasureRequest) => {
      handler(new MessageEvent('message', { data: request }))
    },
  }
}

beforeEach(() => {
  installCanvas()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('measure-worker', () => {
  it('answers with widths for the requested code points', async () => {
    const { posted, send } = await loadWorker()

    send({ requestId: 7, spec, codePoints: new Uint32Array([0x41, 0x42]) })

    expect(posted).toHaveLength(1)
    expect(posted[0].requestId).toBe(7)
    expect(Array.from(posted[0].widths ?? [])).toEqual([1, 1])
    expect(posted[0].error).toBeUndefined()
  })

  it('returns exactly what the shared measuring core produces', async () => {
    // The worker is a shell around `measureCodePoints`; it must not massage the
    // numbers on the way out, or worker and fallback widths would disagree.
    const codePoints = new Uint32Array([0x41, 0x1f600, 0x2500])
    const stubCtx = { font: '', measureText: (text: string) => ({ width: text.length }) }
    const expected = measureCodePoints(stubCtx, spec, codePoints)
    const { posted, send } = await loadWorker()

    send({ requestId: 1, spec, codePoints })

    expect(Array.from(posted[0].widths ?? [])).toEqual(Array.from(expected))
  })

  it('echoes the code points back, so the caller can pair them with the widths', async () => {
    const { posted, send } = await loadWorker()

    send({ requestId: 1, spec, codePoints: new Uint32Array([0x1f600]) })

    expect(Array.from(posted[0].codePoints ?? [])).toEqual([0x1f600])
  })

  it('reports an error against the request ID instead of throwing into the void', async () => {
    // A rejected job has to come back addressed, or the caller waits out its
    // whole timeout for an answer that already failed.
    installCanvas({ contextAvailable: false })
    const { posted, send } = await loadWorker()

    send({ requestId: 42, spec, codePoints: new Uint32Array([0x41]) })

    expect(posted).toHaveLength(1)
    expect(posted[0].requestId).toBe(42)
    expect(posted[0].widths).toBeUndefined()
    expect(posted[0].error).toContain('context unavailable')
  })

  it('handles an empty request without erroring', async () => {
    const { posted, send } = await loadWorker()

    send({ requestId: 2, spec, codePoints: new Uint32Array([]) })

    expect(posted[0].error).toBeUndefined()
    expect(posted[0].widths?.length).toBe(0)
  })
})
