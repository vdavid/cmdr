import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchFeedbackAndErrorsData } from './feedback-and-errors.js'
import {
  countFeedbackWithReplyTo,
  tallyErrorReportsByField,
  errorReportsByDay,
  type FeedbackRow,
  type ErrorReportRow,
} from '../../feedback-and-errors.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = { LICENSE_SERVER_ADMIN_TOKEN: 'test-admin-token' }

const sampleFeedback: FeedbackRow[] = [
  {
    id: 2,
    createdAt: '2026-06-10 09:00:00',
    feedback: 'Love it',
    email: 'a@example.com',
    appVersion: '0.22.0',
    osVersion: 'macOS 15.5',
    buildMode: 'release',
  },
  {
    id: 1,
    createdAt: '2026-06-09 08:00:00',
    feedback: 'A bug report',
    email: null,
    appVersion: '0.22.0',
    osVersion: 'macOS 14.0',
    buildMode: 'release',
  },
]

const sampleErrors: ErrorReportRow[] = [
  {
    id: 'ERR-AAAAA',
    kind: 'auto',
    appVersion: '0.22.0',
    osVersion: 'macOS 15.5',
    arch: 'aarch64',
    date: '2026-06-10',
    generatedAt: '2026-06-10T09:00:00.000Z',
  },
  {
    id: 'ERR-BBBBB',
    kind: 'auto',
    appVersion: '0.22.0',
    osVersion: 'macOS 15.5',
    arch: 'aarch64',
    date: '2026-06-10',
    generatedAt: '2026-06-10T09:05:00.000Z',
  },
  {
    id: 'ERR-CCCCC',
    kind: 'user',
    appVersion: '0.21.0',
    osVersion: 'macOS 14.0',
    arch: 'x86_64',
    date: '2026-06-09',
    generatedAt: '2026-06-09T08:00:00.000Z',
  },
]

describe('countFeedbackWithReplyTo', () => {
  it('counts only rows with a non-empty email', () => {
    expect(countFeedbackWithReplyTo(sampleFeedback)).toBe(1)
    expect(countFeedbackWithReplyTo([{ ...sampleFeedback[0], email: '' }])).toBe(0)
    expect(countFeedbackWithReplyTo([])).toBe(0)
  })
})

describe('tallyErrorReportsByField', () => {
  it('tallies by kind, highest first', () => {
    expect(tallyErrorReportsByField(sampleErrors, 'kind')).toEqual([
      { key: 'auto', count: 2 },
      { key: 'user', count: 1 },
    ])
  })

  it('tallies by app version', () => {
    expect(tallyErrorReportsByField(sampleErrors, 'appVersion')).toEqual([
      { key: '0.22.0', count: 2 },
      { key: '0.21.0', count: 1 },
    ])
  })

  it('labels missing values rather than dropping them', () => {
    const rows = [{ ...sampleErrors[0], arch: '' }]
    expect(tallyErrorReportsByField(rows, 'arch')).toEqual([{ key: '(unknown)', count: 1 }])
  })
})

describe('errorReportsByDay', () => {
  it('groups by day, oldest first', () => {
    expect(errorReportsByDay(sampleErrors)).toEqual([
      { date: '2026-06-09', count: 1 },
      { date: '2026-06-10', count: 2 },
    ])
  })

  it('handles empty input', () => {
    expect(errorReportsByDay([])).toEqual([])
  })
})

describe('fetchFeedbackAndErrorsData', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    clearMemoryCache()
  })

  it('returns both streams on success', async () => {
    const fetchMock = vi.fn()
    fetchMock.mockImplementation((url: string) => {
      if (String(url).includes('/admin/feedback')) {
        return Promise.resolve({ ok: true, json: async () => sampleFeedback })
      }
      if (String(url).includes('/admin/error-reports')) {
        return Promise.resolve({ ok: true, json: async () => sampleErrors })
      }
      return Promise.resolve({ ok: false, status: 404, text: async () => 'Not found' })
    })
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchFeedbackAndErrorsData(mockEnv, { range: '7d', day: null })
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.data.feedback).toEqual(sampleFeedback)
    expect(result.data.errorReports).toEqual(sampleErrors)
    expect(fetchMock.mock.calls[0][1]?.headers).toEqual({ Authorization: 'Bearer test-admin-token' })
  })

  it('maps the 24h range up to the worker 7d window', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => [] })
    vi.stubGlobal('fetch', fetchMock)

    await fetchFeedbackAndErrorsData(mockEnv, { range: '24h', day: null })
    const urls = fetchMock.mock.calls.map((c) => String(c[0]))
    expect(urls.some((u) => u.includes('/admin/feedback?range=7d'))).toBe(true)
    expect(urls.some((u) => u.includes('/admin/error-reports?range=7d'))).toBe(true)
  })

  it('passes 30d through unchanged', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => [] })
    vi.stubGlobal('fetch', fetchMock)

    await fetchFeedbackAndErrorsData(mockEnv, { range: '30d', day: null })
    const urls = fetchMock.mock.calls.map((c) => String(c[0]))
    expect(urls.every((u) => u.includes('range=30d'))).toBe(true)
  })

  it('returns an error when a worker endpoint fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401, text: async () => 'Unauthorized' }))
    const result = await fetchFeedbackAndErrorsData(mockEnv, { range: '7d', day: null })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Feedback & errors')
    expect(result.error).toContain('401')
  })

  it('returns an error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('DNS resolution failed')))
    const result = await fetchFeedbackAndErrorsData(mockEnv, { range: '7d', day: null })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('DNS resolution failed')
  })
})
