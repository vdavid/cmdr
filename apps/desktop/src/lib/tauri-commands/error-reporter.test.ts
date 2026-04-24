/**
 * Tests for the error-reporter Tauri command wrappers.
 *
 * Each wrapper is a thin `invoke<T>('command_name', args)` call. We assert the wrapper
 * forwards the right command name + args and returns whatever invoke returns.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
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
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-XYZ99' })
      const result = await sendErrorReport('a note')
      expect(invoke).toHaveBeenCalledWith('send_error_report', { userNote: 'a note' })
      expect(result).toEqual({ id: 'ERR-XYZ99' })
    })

    it('forwards undefined when no note is provided', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-XYZ99' })
      await sendErrorReport()
      expect(invoke).toHaveBeenCalledWith('send_error_report', { userNote: undefined })
    })
  })

  describe('saveErrorReportToDisk', () => {
    it('returns the saved file path', async () => {
      vi.mocked(invoke).mockResolvedValueOnce('/some/path/error-report-debug-20260423T100000Z.zip')
      const result = await saveErrorReportToDisk()
      expect(invoke).toHaveBeenCalledWith('save_error_report_to_disk', { userNote: undefined })
      expect(result).toBe('/some/path/error-report-debug-20260423T100000Z.zip')
    })
  })
})
