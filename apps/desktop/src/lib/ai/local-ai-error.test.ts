import { describe, it, expect } from 'vitest'
import { localAiErrorLogLevel, toLocalAiError } from './local-ai-error'

describe('toLocalAiError', () => {
  it('keeps the typed error the backend rejected with', () => {
    expect(toLocalAiError({ type: 'managed', refusal: 'aiOff' })).toEqual({ type: 'managed', refusal: 'aiOff' })
    expect(toLocalAiError({ type: 'cancelled' })).toEqual({ type: 'cancelled' })
    expect(toLocalAiError({ type: 'failed', detail: 'disk full' })).toEqual({ type: 'failed', detail: 'disk full' })
  })

  it('reads anything else (an IPC failure, a stray throw) as a failure', () => {
    expect(toLocalAiError(new Error('IPC down'))).toEqual({ type: 'failed', detail: 'Error: IPC down' })
    expect(toLocalAiError({ type: 'somethingNew' })).toEqual({ type: 'failed', detail: '[object Object]' })
    expect(toLocalAiError(null)).toEqual({ type: 'failed', detail: 'null' })
  })
})

describe('localAiErrorLogLevel', () => {
  it('logs the organization’s refusal and a cancel at info, since an error log can send a report', () => {
    expect(localAiErrorLogLevel({ type: 'managed', refusal: 'aiOff' })).toBe('info')
    expect(localAiErrorLogLevel({ type: 'cancelled' })).toBe('info')
  })

  it('logs a real failure at error', () => {
    expect(localAiErrorLogLevel({ type: 'failed', detail: 'x' })).toBe('error')
    expect(localAiErrorLogLevel({ type: 'unsupported' })).toBe('error')
  })
})
