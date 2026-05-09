/**
 * Tests for the error-reporter Tauri command wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    sendErrorReport: vi.fn(),
    saveErrorReportToDisk: vi.fn(),
  },
}))

import { invoke } from '@tauri-apps/api/core'
import { commands } from '$lib/ipc/bindings'
import { prepareErrorReportPreview, sendErrorReport, saveErrorReportToDisk } from './error-reporter'

describe('error-reporter wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('prepareErrorReportPreview', () => {
    it('forwards undefined when no note is provided', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-AB23X' })
      const result = await prepareErrorReportPreview()
      expect(invoke).toHaveBeenCalledWith('prepare_error_report_preview', { userNote: undefined })
      expect(result).toEqual({ id: 'ERR-AB23X' })
    })

    it('forwards the user note when provided', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-AB23X', sizeBytes: 1234 })
      await prepareErrorReportPreview('Something broke')
      expect(invoke).toHaveBeenCalledWith('prepare_error_report_preview', {
        userNote: 'Something broke',
      })
    })

    it('propagates rejection from invoke', async () => {
      vi.mocked(invoke).mockRejectedValueOnce('preview failed')
      await expect(prepareErrorReportPreview()).rejects.toBe('preview failed')
    })
  })

  describe('sendErrorReport', () => {
    it('forwards the user note and returns the server-issued ID', async () => {
      vi.mocked(commands.sendErrorReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-XYZ99' } })
      const result = await sendErrorReport('a note')
      expect(commands.sendErrorReport).toHaveBeenCalledWith('a note')
      expect(result).toEqual({ id: 'ERR-XYZ99' })
    })

    it('forwards null when no note is provided', async () => {
      vi.mocked(commands.sendErrorReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-XYZ99' } })
      await sendErrorReport()
      expect(commands.sendErrorReport).toHaveBeenCalledWith(null)
    })
  })

  describe('saveErrorReportToDisk', () => {
    it('returns the saved file path', async () => {
      vi.mocked(commands.saveErrorReportToDisk).mockResolvedValueOnce({
        status: 'ok',
        data: '/some/path/error-report-debug-20260423T100000Z.zip',
      })
      const result = await saveErrorReportToDisk()
      expect(commands.saveErrorReportToDisk).toHaveBeenCalledWith(null)
      expect(result).toBe('/some/path/error-report-debug-20260423T100000Z.zip')
    })
  })
})
